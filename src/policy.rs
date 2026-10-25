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


use crate::contract::ContractStatus;
use crate::engine::EngineCycle;
use crate::engine_project::{EngineDesignStatus, EngineProjectId, PropellantPreset};
use crate::flight::Payload;
use crate::game_state::GameState;
use crate::project::{ProjectKind, ProjectRef};
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
/// 1. Hire up to 3 engineering + 4 manufacturing teams.
/// 2. Design a kerolox gas-generator booster engine and a hydrolox
///    upper-stage engine (the hydrolox path exercises the hydrogen
///    problems factor every run).
/// 3. When both engines finish design, start a fixed-template
///    two-stage rocket (kerolox first stage, hydrolox upper).
/// 4. When the rocket design reaches Testing, set an auto-build
///    target of 1 so the game keeps one rocket in inventory.
/// 5. Keep idle engineers testing; whenever a project in Testing has
///    discovered flaws, revise them; accept every uprate offered and
///    revise to apply it. (Skipping revision entirely is
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
    /// What this bot builds and staffs (`MEDIUM` for `basic`).
    template: BotTemplate,
    booster: Option<EngineProjectId>,
    upper: Option<EngineProjectId>,
    rocket: Option<RocketProjectId>,
    auto_build_set: bool,
    /// Standing bid rules installed yet? (Done once; the game's rule
    /// engine does the actual bidding from then on.)
    bid_rules_set: bool,
    /// Markup the policy's standing rules use: bid = cost × (1 + margin).
    bid_margin: f64,
    /// Before the first success, launch only once every project the
    /// template flies has had this many testing cycles in a row without
    /// a discovery and nothing known left unrevised — the "ULA path"
    /// of 22_FLAWS.md §3.10. `None` launches the day a rocket exists.
    launch_after_quiet_cycles: Option<u32>,
}

/// An engine family a bot designs: cycle, propellant and size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BotEngine {
    pub name: &'static str,
    pub cycle: EngineCycle,
    pub preset: PropellantPreset,
    pub scale: f64,
}

/// One stage of a bot's stack: how many engines under how much
/// propellant. Structure comes from the model (`recompute_structural_masses`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BotStage {
    pub engine_count: u32,
    pub propellant_kg: f64,
}

/// What a bot builds and staffs: its engine families, its two-stage
/// stack and its payroll. One `BasicPolicy` loop drives every
/// template; the templates are the strategies the band test exercises
/// (24_STRATEGIES.md §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BotTemplate {
    /// CLI / reporting name.
    pub policy_name: &'static str,
    pub rocket_name: &'static str,
    pub booster: BotEngine,
    /// The upper stage's family; `None` flies the booster family's
    /// vacuum bell, one engine programme for the whole vehicle.
    pub upper: Option<BotEngine>,
    /// First stage, upper stage.
    pub stages: [BotStage; 2],
    pub engineering_teams: usize,
    pub manufacturing_teams: usize,
    /// Markup of the standing bid rules: bid = cost × (1 + this).
    pub bid_margin: f64,
}

/// The basic bot: two kerolox gas-generator boosters under 90 t, a
/// hydrolox expander under 15 t — about 2.5 t to LEO — on three
/// engineering teams and four manufacturing lines. Four lines: with
/// hydrogen engines' build premium (21_ENGINES.md step 5) three left
/// the upper-stage engine the bottleneck; a fifth costs more in salary
/// than it saves in latency.
pub const MEDIUM: BotTemplate = BotTemplate {
    policy_name: "basic",
    rocket_name: "BLV-1",
    booster: BotEngine { name: "BLV Booster", cycle: EngineCycle::GasGenerator, preset: PropellantPreset::Kerolox, scale: 1.0 },
    upper: Some(BotEngine { name: "BLV Upper", cycle: EngineCycle::Expander, preset: PropellantPreset::Hydrolox, scale: 1.0 }),
    stages: [
        BotStage { engine_count: 2, propellant_kg: 90_000.0 },
        BotStage { engine_count: 1, propellant_kg: 15_000.0 },
    ],
    engineering_teams: 3,
    manufacturing_teams: 4,
    bid_margin: DEFAULT_BID_MARGIN,
};

/// The small launcher (24_STRATEGIES.md step 4): nine 25 kN kerolox
/// gas generators under 15.3 t and one of the same family's vacuum
/// bells under 2.1 t — an Electron-shaped 19 t vehicle, ~270 kg to
/// LEO and ~185 kg to SSO — on two engineering teams and two lines.
/// One engine programme, one small rocket, the smallsat market.
pub const SMALLSAT: BotTemplate = BotTemplate {
    policy_name: "smallsat",
    rocket_name: "SLV-1",
    booster: BotEngine { name: "SLV Engine", cycle: EngineCycle::GasGenerator, preset: PropellantPreset::Kerolox, scale: 25.0 / 900.0 },
    upper: None,
    stages: [
        BotStage { engine_count: 9, propellant_kg: 15_300.0 },
        BotStage { engine_count: 1, propellant_kg: 2_100.0 },
    ],
    engineering_teams: 2,
    manufacturing_teams: 2,
    // A thin business: Electron sells at ~25 % over cost.
    bid_margin: 0.25,
};

/// A Firefly Alpha–shaped vehicle for comparison: four 190 kN kerolox
/// gas generators under 53 t and one of the family's vacuum bells
/// under 5.8 t — ~1 t to LEO, ~700 kg to SSO.
pub const ALPHA: BotTemplate = BotTemplate {
    policy_name: "alpha",
    rocket_name: "ALV-1",
    booster: BotEngine { name: "ALV Engine", cycle: EngineCycle::GasGenerator, preset: PropellantPreset::Kerolox, scale: 190.0 / 900.0 },
    upper: None,
    stages: [
        BotStage { engine_count: 4, propellant_kg: 53_400.0 },
        BotStage { engine_count: 1, propellant_kg: 5_800.0 },
    ],
    engineering_teams: 2,
    manufacturing_teams: 2,
    bid_margin: 0.5,
};

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
    /// (or pending improvements / tech deficiencies for engines). The
    /// bot takes every uprate it is offered (21_ENGINES.md step 7) and
    /// revises to apply it, accepting the flaw risk.
    fn revise_discovered_flaws(game: &mut GameState) {
        let company = &mut game.player_company;
        let offered: Vec<ProjectRef> = company.engine_projects.iter()
            .filter(|ep| ep.offered_uprates().next().is_some())
            .map(|ep| ProjectRef::Engine(ep.project_id))
            .collect();
        let mut accepted: Vec<ProjectRef> = Vec::new();
        for r in offered {
            if company.accept_uprates(r).is_some_and(|(_, n)| n > 0) {
                accepted.push(r);
            }
        }
        // Engines and rockets only; the bot doesn't run reactors.
        let due: Vec<ProjectRef> = company.projects()
            .filter(|p| matches!(p.kind(), ProjectKind::Engine | ProjectKind::Rocket))
            .filter(|p| matches!(p.status(), EngineDesignStatus::Testing { .. })
                && (p.discovered_flaw_count() > 0 || accepted.contains(&p.project_ref())))
            .map(|p| p.project_ref())
            .collect();
        for r in due {
            company.start_revision(r);
        }
    }
}

/// Don't hire or start projects when cash falls below this floor.
const MONEY_FLOOR: f64 = 5_000_000.0;
/// Fraction of computed max payload the bot is willing to book —
/// shared with the game's rule engine.
/// Default markup for the policy's standing bid rules. After the M4
/// cost retune (material prices ×4) a vehicle costs ~40-60% of a
/// winning bid, so cost × 2 is market-rate pricing: the 2026-07 M4
/// sweep landed payments at the same real-world prices as before the
/// retune (~$31M avg) with a 50% cost ratio. Higher markups still
/// win in the uncontested small-payload market until budget ceilings
/// push back (1.25 → avg payment $35M), so this locks an honest
/// posture, not an optimum.
const DEFAULT_BID_MARGIN: f64 = 1.0;

impl BasicPolicy {
    pub fn new() -> Self {
        Self::with_margin(DEFAULT_BID_MARGIN)
    }

    /// A BasicPolicy whose standing rules use the given markup —
    /// the knob the margin sweep exercises (`--policy basic:0.15`).
    pub fn with_margin(bid_margin: f64) -> Self {
        Self::with_template(BotTemplate { bid_margin, ..MEDIUM })
    }

    /// The basic loop on a template (`--policy smallsat`, …).
    pub fn with_template(template: BotTemplate) -> Self {
        BasicPolicy {
            template,
            booster: None,
            upper: None,
            rocket: None,
            auto_build_set: false,
            bid_rules_set: false,
            bid_margin: template.bid_margin,
            launch_after_quiet_cycles: None,
        }
    }

    /// A prudent programme: more engineers, and no first launch until
    /// every design has gone `quiet_cycles` test cycles without
    /// finding anything (`--policy prudent`, `prudent:<teams>,<cycles>`).
    pub fn prudent(engineering_teams: usize, quiet_cycles: u32) -> Self {
        BasicPolicy {
            template: BotTemplate { engineering_teams, ..MEDIUM },
            launch_after_quiet_cycles: Some(quiet_cycles),
            ..Self::new()
        }
    }

    /// Whether the prudent gate is open: every project the template
    /// flies is in Testing, has nothing discovered and unrevised, and
    /// has been quiet for the required cycles.
    fn ready_to_launch(&self, game: &GameState) -> bool {
        let Some(quiet) = self.launch_after_quiet_cycles else { return true };
        let company = &game.player_company;
        let engines_ready = [self.booster, self.upper].iter().flatten().all(|id| {
            company.engine_projects.iter().find(|ep| ep.project_id == *id).is_some_and(|ep| {
                matches!(ep.status, EngineDesignStatus::Testing { .. })
                    && ep.discovered_flaw_count() == 0 && ep.quiet_cycles >= quiet
            })
        });
        let rocket_ready = self.rocket.is_some_and(|id| {
            company.rocket_projects.iter().find(|rp| rp.project_id == id).is_some_and(|rp| {
                matches!(rp.status, RocketDesignStatus::Testing { .. })
                    && rp.discovered_flaw_count() == 0 && rp.quiet_cycles >= quiet
            })
        });
        engines_ready && rocket_ready
    }

    /// Bid campaign blocks at the same markup as the standing rules.
    /// Blocks are deliberately NOT auto-bid by the rule engine — a
    /// human decides them in the Programs modal after the announcement
    /// pause — so the sim bot places its own. Gate per the plan's Q2
    /// answer: a capable Testing design with cost history and at least
    /// one capable vehicle free beyond accepted work.
    fn bid_campaign_blocks(&mut self, game: &mut GameState) {
        let soliciting: Vec<(crate::contract::CampaignId, String, f64)> =
            game.active_campaigns.iter()
                .filter_map(|c| match c.status {
                    crate::contract::CampaignStatus::Soliciting { player_bid: None, .. } =>
                        Some((c.id, c.destination.clone(), c.payload_kg)),
                    _ => None,
                })
                .collect();
        if soliciting.is_empty() {
            return;
        }
        for (id, dest, payload_kg) in soliciting {
            let (capable_projects, best_cost) = game.player_capable_cost(&dest, payload_kg);
            let Some(cost) = best_cost else { continue };
            // The same readiness gate the player's standing rules use.
            if game.free_capable_stock(&capable_projects) == 0 {
                continue;
            }
            let bid = crate::contract::round_price(cost * (1.0 + self.bid_margin));
            if bid <= 0.0 {
                continue;
            }
            game.place_campaign_bid(id, bid);
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
        if game.player_company.teams.len() < self.template.engineering_teams {
            let name = format!("Team {}", game.player_company.teams.len() + 1);
            if let Some(evt) = game.player_company.hire_team(name, &game.balance) {
                game.log(evt);
            }
        }
        if game.player_company.manufacturing_teams.len() < self.template.manufacturing_teams {
            let name = format!("Mfg Team {}", game.player_company.manufacturing_teams.len() + 1);
            if let Some(evt) = game.player_company.hire_manufacturing_team(name, &game.balance) {
                game.log(evt);
            }
        }
    }

    fn ensure_engine_projects(&mut self, game: &mut GameState) {
        if game.player_company.money < MONEY_FLOOR {
            return;
        }
        if self.booster.is_none() {
            let e = self.template.booster;
            if let Some(evt) = game.player_company.start_engine_project(
                e.name.into(), e.cycle, e.preset, e.scale, None, &game.balance,
            ) {
                game.log(evt);
                self.booster = game.player_company.engine_projects.last()
                    .map(|p| p.project_id);
            }
        }
        if let (None, Some(e)) = (self.upper, self.template.upper) {
            if let Some(evt) = game.player_company.start_engine_project(
                e.name.into(), e.cycle, e.preset, e.scale, None, &game.balance,
            ) {
                game.log(evt);
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
        let rocket = self.rocket.map(ProjectRef::Rocket);
        if let Some((r, status)) = rocket.and_then(|r| company.project(r).map(|p| (r, p.status().clone()))) {
            let want = match status {
                RocketDesignStatus::InDesign { .. }
                | RocketDesignStatus::Revising { .. } => 2,
                RocketDesignStatus::Testing { .. } => 1,
                RocketDesignStatus::Proposed { .. } => 0,
            };
            let staffed = |c: &crate::company::Company| c.project(r).map_or(0, |p| p.teams_assigned());
            while staffed(company) < want && company.add_team(r) {}
            // Pull a team off an engine if the rocket is starved.
            if staffed(company) == 0 {
                company.steal_team_to(r);
            }
        }
        let idle_engines: Vec<ProjectRef> = company.engine_projects.iter()
            .filter(|p| p.teams_assigned == 0)
            .map(|p| ProjectRef::Engine(p.project_id))
            .collect();
        for r in idle_engines {
            company.add_team(r);
        }
    }

    /// Fixed two-stage template: one kerolox booster engine under a
    /// 60 t first stage, one hydrolox engine under an 8 t upper stage.
    /// Sized to put a small-sat class payload into LEO with margin.
    ///
    /// Sized for the ascent model. The first stage was 42 t until the
    /// planner started charging gravity losses, then 60 t on one engine
    /// to buy a near-horizontal handover under the pure gravity turn —
    /// which turned out to be that model levelling a TWR-1.22 vehicle off
    /// at 11 km. With the pitch program (17_3_PHYSICS.md D7) staging at
    /// 65–85 km, the same template lifted 1.7 t to LEO instead of 2.8 t,
    /// and adding propellant to one engine only made it slower and worse.
    /// Two booster engines under 90 t and a 15 t upper stage put it back
    /// at ~3.4 t to LEO; GEO stays lower (~200 kg vs 380) because the
    /// upper stage now pays real gravity on the way up.
    fn build_template(&self, game: &GameState) -> Option<(String, Vec<Vec<Stage>>)> {
        let company = &game.player_company;
        let booster = company.engine_projects.iter()
            .find(|p| Some(p.project_id) == self.booster)?;
        // The upper stage's family, or the booster's when the template
        // flies one programme's two bells.
        let upper = match self.template.upper {
            Some(_) => company.engine_projects.iter().find(|p| Some(p.project_id) == self.upper)?,
            None => booster,
        };
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

        let [first, second] = self.template.stages;
        let s1 = Stage {
            id: StageId(1),
            name: format!("{} S1", self.template.rocket_name),
            // First stage lights at sea level; upper stage flies the
            // vacuum bell of its own engine family.
            engine: booster.design_variant(false, &game.balance.nozzle),
            engine_count: first.engine_count,
            propellant_mass_kg: first.propellant_kg,
            structural_mass_kg: 0.0, // from the model below
            fairing: None,
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2),
            name: format!("{} S2", self.template.rocket_name),
            engine: upper.design_variant(true, &game.balance.nozzle),
            engine_count: second.engine_count,
            propellant_mass_kg: second.propellant_kg,
            structural_mass_kg: 0.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        // Structure from the model the designer uses, so the bot's
        // vehicle tracks it (24_STRATEGIES.md step 3) instead of the
        // 6 450 / 1 500 kg it carried when the model was heavier.
        let mut groups = vec![vec![s1], vec![s2]];
        crate::stage::recompute_structural_masses(&mut groups);
        // Cover housekeeping power like the designer's default panels —
        // sized after the structure, which sets the demand.
        for stage in groups.iter_mut().flatten() {
            let panel = crate::power::solar_panel_for_stage_demand(stage);
            stage.power_sources.push(panel);
        }
        Some((self.template.rocket_name.into(), groups))
    }

    fn maybe_design_rocket(&mut self, game: &mut GameState) {
        if self.rocket.is_some() || game.player_company.money < MONEY_FLOOR {
            return;
        }
        let Some((name, stage_groups)) = self.build_template(game) else {
            return;
        };
        if let Some(evt) = game.player_company.start_rocket_project(name, stage_groups, &game.balance) {
            game.log(evt);
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

    /// Whether the template can serve a delivery, by the game's own
    /// rule — the one the bid engine and the contract colours use.
    fn template_can_serve(&self, game: &GameState, dest: &str, payload_kg: f64) -> bool {
        self.rocket
            .and_then(|rid| game.player_company.rocket_projects.iter()
                .find(|p| p.project_id == rid))
            .is_some_and(|p| game.project_can_serve(p, dest, payload_kg))
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
            .inventory.rockets.iter().map(|r| r.item_id)
            .find(|id| game.rocket_grounded_until(*id).is_none())
        else {
            return;
        };

        // Until the first fully successful flight, fly test masses —
        // one vehicle in the air at a time. Failures discover flaws
        // (which the revision loop then fixes) without burning
        // contracts or wrecking reputation beyond recovery.
        if !Self::had_first_success(game) {
            if Self::flight_in_transit(game) || !self.ready_to_launch(game) {
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
                    if !self.template_can_serve(game, &dest, payload_kg) {
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
        self.bid_campaign_blocks(game);
        self.accept_and_launch(game);
    }

    fn name(&self) -> &'static str {
        if self.launch_after_quiet_cycles.is_some() { "prudent" } else { self.template.policy_name }
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
    if let Some(rest) = name.strip_prefix("prudent:") {
        let (teams, cycles) = rest.split_once(',')?;
        return Some(Box::new(BasicPolicy::prudent(teams.parse().ok()?, cycles.parse().ok()?)));
    }
    // `<template>:<engineering teams>,<manufacturing teams>[,<S1 kg>,<S2 kg>]`
    // — the payroll and tank sweep for a templated bot.
    for (prefix, base) in [("smallsat:", SMALLSAT), ("alpha:", ALPHA)] {
        if let Some(rest) = name.strip_prefix(prefix) {
            let parts: Vec<&str> = rest.split(',').collect();
            if parts.len() != 2 && parts.len() != 4 {
                return None;
            }
            let mut template = BotTemplate {
                engineering_teams: parts[0].parse().ok()?,
                manufacturing_teams: parts[1].parse().ok()?,
                ..base
            };
            if parts.len() == 4 {
                template.stages[0].propellant_kg = parts[2].parse().ok()?;
                template.stages[1].propellant_kg = parts[3].parse().ok()?;
            }
            return Some(Box::new(BasicPolicy::with_template(template)));
        }
    }
    match name {
        "none" => Some(Box::new(NullPolicy)),
        "basic" => Some(Box::new(BasicPolicy::new())),
        "prudent" => Some(Box::new(BasicPolicy::prudent(6, 3))),
        "smallsat" => Some(Box::new(BasicPolicy::with_template(SMALLSAT))),
        "alpha" => Some(Box::new(BasicPolicy::with_template(ALPHA))),
        _ => None,
    }
}

/// Names accepted by `policy_by_name`, for CLI help text.
pub const POLICY_NAMES: &[&str] = &[
    "none", "basic", "basic:<margin>", "prudent", "prudent:<teams>,<cycles>",
    "smallsat", "smallsat:<eng teams>,<mfg teams>",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: u64, days: u32) -> (GameState, BasicPolicy) {
        let mut policy = BasicPolicy::new();
        let mut gs = GameState::new("BotCorp".into(), seed);
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
        // Testing or mid-revision: both mean the design is through
        // design and in the test-revise loop.
        assert!(
            matches!(rp.status, RocketDesignStatus::Testing { .. } | RocketDesignStatus::Revising { .. }),
            "rocket should reach Testing within 2 years, got {:?}", rp.status,
        );
        assert!(policy.auto_build_set, "auto-build should be enabled");
    }

    #[test]
    fn test_basic_policy_template_lifts_smallsats_to_leo() {
        let (gs, policy) = run(42, 730);
        assert!(policy.template_can_serve(&gs, "leo", 500.0),
            "template should serve a 500 kg smallsat to LEO by the game's own rule");
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
