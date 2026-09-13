//! Key handling for the main tabs: the project panes, manufacturing,
//! contracts and launches, and the manifest launch that closes the loop.

use super::*;

impl App {
    /// Assemble the launch manifest from the user's checks and submit it.
    /// All picked contracts must share a destination; the destination of
    /// the carrier flight is that shared destination (or LEO if the only
    /// picks are spacecraft / nothing). Spacecraft payloads are taken from
    /// inventory at submit time and packed with `deploy_at = destination`.
    pub(super) fn submit_manifest_launch(
        &mut self,
        rocket_item_id: crate::manufacturing::InventoryItemId,
        persist: bool,
        contract_picks: Vec<bool>,
        spacecraft_picks: Vec<bool>,
        spacecraft_item_ids: Vec<crate::manufacturing::InventoryItemId>,
    ) {
        use crate::game_state::ManifestError;

        let contract_indices: Vec<usize> = contract_picks.iter().enumerate()
            .filter(|(_, picked)| **picked)
            .map(|(i, _)| i)
            .collect();
        let picked_spacecraft: Vec<crate::manufacturing::InventoryItemId> =
            spacecraft_picks.iter().enumerate()
                .filter(|(_, picked)| **picked)
                .map(|(i, _)| spacecraft_item_ids[i])
                .collect();

        let (destination, payloads) = match self.game
            .build_launch_payloads(&contract_indices, &picked_spacecraft)
        {
            Ok(dp) => dp,
            Err(ManifestError::ConflictingDestinations { first, second }) => {
                self.status_message = Some(format!(
                    "Picked contracts have different destinations ({} vs {}). Untoggle one.",
                    first, second,
                ));
                return;
            }
            Err(ManifestError::SpacecraftMissing) => {
                self.status_message = Some("Spacecraft payload no longer in inventory.".into());
                return;
            }
            Err(ManifestError::PayloadProjectMissing) => {
                self.status_message = Some("Payload rocket project not found.".into());
                return;
            }
        };

        match self.game.launch_rocket(rocket_item_id, &destination, payloads, persist) {
            Some((_events, Some(record))) => {
                self.input_mode = InputMode::LaunchResult { record };
            }
            Some((_events, None)) => {
                self.status_message = Some("Flight departed — in transit".into());
                self.exit_modal();
            }
            None => {
                self.status_message = Some("Launch failed (rocket not found)".into());
                self.exit_modal();
            }
        }
    }

    /// The project the pane's cursor is on. Selections index the visible
    /// list (drafts and retired designs hidden), and every action wants
    /// the id — a row can slide under the cursor across a day tick.
    pub(super) fn selected_project(&self, kind: ProjectKind) -> Option<ProjectRef> {
        self.game.player_company.visible_projects(kind)
            .nth(self.selected_item)
            .map(|p| p.project_ref())
    }

    /// The Engines, Rockets and Reactors panes: the keys all three share
    /// first, then the few each pane has of its own.
    pub(super) fn handle_project_pane_key(&mut self, kind: ProjectKind, key: KeyCode) {
        let selected = self.selected_project(kind);
        let noun = kind.noun();
        let company = &mut self.game.player_company;
        match key {
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.status_message = Some(match selected.and_then(|r| company.toggle_auto_revise(r)) {
                    Some((name, on)) =>
                        format!("{}: auto-revise {}", name, if on { "on" } else { "off" }),
                    None => format!("No {} selected", noun),
                });
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Add an idle team, or steal one from the busiest project.
                self.status_message = Some(if selected.is_some_and(|r| company.add_team(r)) {
                    "Team assigned".into()
                } else if let Some(from) = selected.and_then(|r| company.steal_team_to(r)) {
                    format!("Team reassigned from {}", from)
                } else {
                    "No teams to reassign".into()
                });
            }
            KeyCode::Char('-') => {
                if selected.is_some_and(|r| company.remove_team(r)) {
                    self.status_message = Some("Team removed".into());
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Revise every discovered flaw, pending improvement and
                // unsolved deficiency. Testing-only.
                self.status_message = Some(match selected.and_then(|r| company.start_revision(r)) {
                    Some(plan) => plan.describe(),
                    None => format!(
                        "Nothing to revise (needs a Testing {} with discovered flaws, \
                         improvements, or deficiencies)", noun),
                });
            }
            KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Delete => {
                match selected {
                    Some(r) => self.confirm_retire(r),
                    None => self.status_message = Some(format!("No {} selected", noun)),
                }
            }
            _ => self.handle_project_pane_specific_key(kind, key, selected),
        }
    }

    /// The keys that genuinely differ between the three project panes.
    pub(super) fn handle_project_pane_specific_key(
        &mut self, kind: ProjectKind, key: KeyCode, selected: Option<ProjectRef>,
    ) {
        let index = selected.and_then(|r| self.game.player_company.list_index(r));
        match (kind, key) {
            // ── Engines ──
            (ProjectKind::Engine, KeyCode::Char('n') | KeyCode::Char('N')) => {
                // Design a new engine standalone (parity with the Reactors
                // pane). Seeds a Proposed engine with sensible defaults and
                // opens the editor with no designer state; 'd' commits it
                // to InDesign, Esc discards the draft. Engines can also
                // still be designed inside the rocket designer.
                use crate::engine_project::DEFAULT_SCALE;
                let n = self.game.player_company.engine_projects.len() + 1;
                let name = format!("Engine Mk{}", n);
                let preset = PropellantPreset::Kerolox;
                let tech_id = crate::technology::technology_for_preset(preset);
                match self.game.player_company.start_proposed_engine_project(
                    name, EngineCycle::GasGenerator, preset, DEFAULT_SCALE, tech_id,
                    &self.game.balance,
                ) {
                    Some(pid) => {
                        self.enter_modal(InputMode::EngineEditor {
                            project_id: pid, cursor: 0, state: None,
                        });
                    }
                    None => {
                        self.status_message = Some("Failed to create engine".into());
                    }
                }
            }
            (ProjectKind::Engine, KeyCode::Char('b') | KeyCode::Char('B')) => {
                // Buy third-party engine
                if self.game.player_company.third_party_catalog.is_empty() {
                    self.status_message = Some(
                        "No third-party engines on the market".into());
                } else {
                    self.enter_modal(InputMode::SelectThirdParty { selected: 0 });
                }
            }
            (ProjectKind::Engine, KeyCode::Char('o') | KeyCode::Char('O')) => {
                // Order standalone engine build
                let idx = index.unwrap_or(usize::MAX);
                if let Some((cost, evt)) = self.game.player_company.order_engine_build(idx, &self.game.balance) {
                    self.game.log(evt);
                    self.status_message = Some(format!("Engine build ordered ({})", crate::resources::format_money(cost)));
                } else {
                    self.status_message = Some("Must be in Testing to order build".into());
                }
            }
            // ── Engines and Rockets: hire ──
            (ProjectKind::Engine | ProjectKind::Rocket, KeyCode::Char('e') | KeyCode::Char('E')) => {
                let team_num = self.game.player_company.team_count() + 1;
                let name = format!("Team {}", team_num);
                if let Some(evt) = self.game.player_company.hire_team(name.clone(), &self.game.balance) {
                    self.game.log(evt);
                    self.status_message = Some(format!("Hired {}", name));
                }
            }
            // ── Reactors ──
            (ProjectKind::Reactor, KeyCode::Char('n') | KeyCode::Char('N')) => {
                use crate::reactor::{EnrichmentLevel, DEFAULT_SCALE};
                // Pick a fresh default name based on how many projects
                // exist; the player can rename inside the editor.
                let n = self.game.player_company.reactor_projects.len() + 1;
                let name = format!("Reactor Mk{}", n);
                let pid = self.game.player_company.start_proposed_reactor(
                    name, DEFAULT_SCALE, EnrichmentLevel::Leu, &self.game.balance,
                );
                self.enter_modal(InputMode::ReactorEditor { project_id: pid, cursor: 0 });
            }
            (ProjectKind::Reactor, KeyCode::Char('e') | KeyCode::Char('E')) => {
                // Re-open the editor on an InDesign reactor. Testing is
                // read-only; Revising is handled by [R]; the pane hides
                // Proposed.
                let Some(ProjectRef::Reactor(pid)) = selected else { return };
                let Some(project) = self.game.player_company.find_reactor_project(pid) else { return };
                if matches!(project.status, crate::project::DesignStatus::InDesign { .. }) {
                    self.enter_modal(InputMode::ReactorEditor { project_id: pid, cursor: 0 });
                } else {
                    self.status_message = Some(
                        "Editor only available on In Design reactors".into());
                }
            }
            // ── Rockets ──
            (ProjectKind::Rocket, KeyCode::Char('n') | KeyCode::Char('N')) => {
                // Start new rocket design flow
                self.enter_modal(InputMode::RocketName { buffer: String::new() });
            }
            (ProjectKind::Rocket, KeyCode::Char('o') | KeyCode::Char('O')) => {
                // Order rocket build
                let idx = index.unwrap_or(usize::MAX);
                if let Some((cost, evt)) = self.game.player_company.order_rocket_build(idx, &self.game.balance) {
                    self.game.log(evt);
                    self.status_message = Some(format!("Build ordered ({})", crate::resources::format_money(cost)));
                } else {
                    self.status_message = Some("Must be in Testing to order build".into());
                }
            }
            (ProjectKind::Rocket, KeyCode::Char('M')) => {
                // Modify the selected rocket project — opens the rocket
                // designer in Modify mode (only propellant + power
                // editable). Only allowed for InDesign / Testing.
                let Some(ProjectRef::Rocket(pid)) = selected else { return };
                let Some(project) = self.game.player_company.rocket_projects.iter()
                    .find(|p| p.project_id == pid) else { return };
                if let RocketDesignStatus::Revising { .. } = &project.status {
                    self.status_message = Some(
                        "Can't modify while revising — finish flaws first".into());
                    return;
                }
                let state = Box::new(RocketDesignerState::from_existing(
                    project, &self.game.player_company,
                ));
                self.enter_modal(InputMode::designer(state));
            }
            (ProjectKind::Rocket, KeyCode::Char('m')) if index.is_some() => {
                // Cycle auto-build target: 0 → 1 → 2 → 3 → 0
                match self.game.player_company.cycle_auto_build_target(index.unwrap_or(usize::MAX)) {
                    Some(0) => self.status_message = Some("Auto-build: off".into()),
                    Some(n) => self.status_message = Some(format!("Auto-build: {}", n)),
                    None => self.status_message =
                        Some("Must be in Testing to set auto-build".into()),
                }
            }
            _ => {}
        }
    }

    /// Open the retire confirmation for `target`, or explain why it
    /// can't be retired. Shared by all three design panes so the prompt
    /// and the refusal messages read the same wherever you press `X`.
    pub(super) fn confirm_retire(&mut self, target: ProjectRef) {
        use crate::company::RetireRefusal;
        match self.game.player_company.retirement_plan(target) {
            Ok(effects) => {
                self.enter_modal(InputMode::ConfirmRetire { target, effects });
            }
            Err(RetireRefusal::EngineInUse { rockets }) => {
                // Naming the blockers makes the fix obvious: retire the
                // rockets first, then the engine goes quietly.
                self.status_message = Some(format!(
                    "Still used by {} — retire {} first",
                    rockets.join(", "),
                    if rockets.len() == 1 { "it" } else { "those" },
                ));
            }
            Err(RetireRefusal::NotFound) => {
                self.status_message = Some("Nothing selected".into());
            }
        }
    }

    /// The order the Manufacturing cursor is on. Selection indexes the
    /// drawn tree, not the queue's own order.
    pub(super) fn selected_manufacturing_order(&self) -> Option<usize> {
        self.game.player_company.manufacturing_display_order()
            .get(self.selected_item)
            .map(|row| row.order_index)
    }

    pub(super) fn handle_manufacturing_key(&mut self, key: KeyCode) {
        // Selection indexes the drawn tree; every order-specific key below
        // needs the queue index it stands for. `usize::MAX` for "nothing
        // selected" lets the company-side bounds checks reject it.
        let order_index = self.selected_manufacturing_order().unwrap_or(usize::MAX);
        match key {
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.game.player_company.add_team_to_manufacturing_order(order_index) {
                    self.status_message = Some("Mfg team assigned".into());
                } else if let Some(from) = self.game.player_company.steal_manufacturing_team_to_order(order_index) {
                    self.status_message = Some(format!("Mfg team reassigned from {}", from));
                } else {
                    self.status_message = Some("No mfg teams to reassign".into());
                }
            }
            KeyCode::Char('-') => {
                if self.game.player_company
                    .remove_team_from_manufacturing_order(order_index)
                {
                    self.status_message = Some("Mfg team removed".into());
                } else {
                    self.status_message = Some(
                        "No team assigned to that build order".into());
                }
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                let team_num = self.game.player_company.manufacturing_teams.len() + 1;
                let name = format!("Mfg Team {}", team_num);
                if let Some(evt) = self.game.player_company.hire_manufacturing_team(name.clone(), &self.game.balance) {
                    self.game.log(evt);
                    self.status_message = Some(format!("Hired {}", name));
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Rush the selected order's rocket. Reassigns immediately
                // rather than waiting for the tick, because the whole point
                // is a deadline.
                let company = &mut self.game.player_company;
                let target = company.manufacturing.orders.get(order_index)
                    .and_then(|o| o.parent_rocket());
                let Some(rp_id) = target else {
                    self.status_message = Some(
                        "That order isn't building toward a rocket".into());
                    return;
                };
                let name = company.rocket_projects.iter()
                    .find(|rp| rp.project_id == rp_id)
                    .map_or_else(|| "rocket".to_string(), |rp| rp.design.name.clone());
                if company.rush_projects.remove(&rp_id) {
                    self.status_message = Some(format!("{name}: rush cancelled"));
                } else {
                    company.rush_projects.insert(rp_id);
                    self.status_message = Some(format!(
                        "{name}: RUSH — the floor drops everything else"));
                }
                company.assign_manufacturing_teams();
            }
            _ => {}
        }
    }

    pub(super) fn handle_contracts_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('a') | KeyCode::Char('A')
            | KeyCode::Char('b') | KeyCode::Char('B')
            | KeyCode::Enter => {
                let avail_len = self.game.available_contracts.len();
                if self.selected_item >= avail_len {
                    self.status_message = Some("Already accepted".into());
                    return;
                }
                if self.game.available_contracts[self.selected_item].is_solicitation() {
                    // Sealed bid: open the price-entry modal, seeded
                    // with any pending bid so it can be revised.
                    let buffer = self.game.available_contracts[self.selected_item]
                        .player_bid
                        .map(|b| format!("{}", b / 1_000_000.0))
                        .unwrap_or_default();
                    self.enter_modal(InputMode::BidEntry {
                        contract_index: self.selected_item,
                        buffer,
                    });
                } else {
                    // Pre-priced contract (campaign mission / legacy
                    // save): accept directly.
                    if let Some(evt) = self.game.accept_contract(self.selected_item) {
                        self.status_message = Some(format!("{}", evt));
                    }
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.enter_modal(InputMode::BidRules { selected: 0 });
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.enter_modal(InputMode::AwardHistory { scroll: 0 });
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.enter_modal(InputMode::Campaigns { selected: 0 });
            }
            _ => {}
        }
    }

    pub(super) fn handle_launches_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Fly a spacecraft to a new destination
                if self.game.spacecraft.is_empty() {
                    self.status_message = Some("No spacecraft available".into());
                    return;
                }
                self.enter_modal(InputMode::FlySelectSpacecraft {
                    selected: 0,
                });
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Dock one spacecraft onto another at the same location.
                if self.game.spacecraft.len() < 2 {
                    self.status_message = Some("Need at least two spacecraft to dock".into());
                    return;
                }
                self.enter_modal(InputMode::DockSelectSmall { selected: 0 });
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                // Undock a payload from a carrier.
                let candidates: Vec<usize> = self.game.spacecraft.iter().enumerate()
                    .filter(|(_, sc)| sc.payloads.iter().any(|p|
                        matches!(p, crate::flight::Payload::Spacecraft { .. })))
                    .map(|(i, _)| i)
                    .collect();
                if candidates.is_empty() {
                    self.status_message = Some("No spacecraft with docked payloads".into());
                    return;
                }
                self.enter_modal(InputMode::UndockSelectCarrier {
                    candidates, selected: 0,
                });
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // Open delta-v planner setup
                let eligible: Vec<usize> = self.game.player_company.rocket_projects.iter()
                    .enumerate()
                    .filter(|(_, rp)| matches!(rp.status, RocketDesignStatus::Testing { .. })
                        && !rp.retired)
                    .map(|(i, _)| i)
                    .collect();
                if eligible.is_empty() {
                    self.status_message = Some("No rocket design available".into());
                    return;
                }
                let locations: Vec<(&'static str, &'static str)> = DELTA_V_MAP.locations().iter()
                    .map(|loc| (loc.id, loc.display_name))
                    .collect();
                // Default to earth_surface
                let default_loc = locations.iter().position(|(id, _)| *id == "earth_surface").unwrap_or(0);
                self.enter_modal(InputMode::PlannerSetup {
                    state: Box::new(PlannerSetupState {
                        eligible_projects: eligible,
                        selected_project: 0,
                        locations,
                        selected_location: default_loc,
                        payload_buffer: "0".into(),
                        active_field: PlannerSetupField::Design,
                    }),
                });
            }
            KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Enter
            | KeyCode::Char('k') | KeyCode::Char('K') => {
                // 'k'/'K' = keep: the carrier becomes a Spacecraft at the
                // destination instead of being discarded on arrival.
                let persist = matches!(key, KeyCode::Char('k') | KeyCode::Char('K'));
                // Launch the selected rocket. Indexes the drawn order, not
                // the inventory's build order.
                let rockets = self.game.player_company.ready_rockets_in_display_order();
                let Some(rocket) = rockets.get(self.selected_item) else {
                    self.status_message = Some("No rocket selected".into());
                    return;
                };
                let item_id = rocket.item_id;
                let project_id = rocket.rocket_project_id;

                // Find the rocket project to get its design
                let rp = self.game.player_company.rocket_projects.iter()
                    .find(|rp| rp.project_id == project_id);
                if rp.is_none() {
                    self.status_message = Some("Rocket project not found".into());
                    return;
                }

                // Enter launch modal — assemble multi-payload manifest.
                let contract_picks = vec![false; self.game.player_company.active_contracts.len()];
                // Same order as the Ready Rockets list it was picked from.
                let spacecraft_item_ids: Vec<_> =
                    self.game.player_company.ready_rockets_in_display_order()
                        .into_iter()
                        .filter(|r| r.item_id != item_id)
                        .map(|r| r.item_id)
                        .collect();
                let spacecraft_picks = vec![false; spacecraft_item_ids.len()];
                self.enter_modal(InputMode::LaunchManifest {
                    rocket_item_id: item_id,
                    persist,
                    contract_picks,
                    spacecraft_picks,
                    spacecraft_item_ids,
                    cursor: 0,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod reactor_render_tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use crate::flaw::{Flaw, FlawConsequence, FlawId, FlawTrigger};
    use crate::reactor::{EnrichmentLevel, ReactorId};
    use crate::reactor_project::{
        ReactorDesignStatus, ReactorImprovement, ReactorImprovementKind, ReactorProject,
        ReactorProjectId,
    };

    /// Headless render smoke test: the Reactors pane draws a project that
    /// exercises every detail branch (discovered flaw, actualized +
    /// pending improvement, tech deficiencies) without panicking, and the
    /// reactor-flavored strings reach the buffer.
    #[test]
    fn reactors_pane_renders_full_detail() {
        let mut game = crate::game_state::GameState::with_money("Render Test".into(), 100_000_000.0, 7);

        let mut project = ReactorProject::new(
            ReactorProjectId(1), ReactorId(1), "Mk1 Reactor".into(), 1.0, EnrichmentLevel::Leu,
            &crate::balance_config::BalanceConfig::default(),
        );
        project.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
        project.cumulative_testing_work = 120.0;
        project.flaws.push(Flaw {
            id: FlawId(1),
            description: "Coolant loop flow restriction".into(),
            consequence: FlawConsequence::PerformanceDegradation(0.07),
            activation_chance: 0.1,
            discovery_probability: 0.5,
            discovered: true,
            trigger: FlawTrigger::PerFlight,
        });
        project.improvements.push(ReactorImprovement {
            id: crate::project::ImprovementId(0),
            description: "Compact reactor core design".into(),
            kind: ReactorImprovementKind::Mass(0.03),
            actualized: true,
        });
        project.improvements.push(ReactorImprovement {
            id: crate::project::ImprovementId(1),
            description: "Optimized coolant flow raises output".into(),
            kind: ReactorImprovementKind::Power(0.02),
            actualized: false,
        });
        // Attach the fission-reactor tech's real deficiency ids so the
        // deficiency block renders against live technology data.
        let def_ids: Vec<_> = game.technologies.iter()
            .find(|t| t.id == crate::technology::TECH_FISSION_REACTOR).unwrap()
            .deficiencies.iter().map(|d| d.id).collect();
        project.tech_deficiency_ids = def_ids;
        game.player_company.reactor_projects.push(project);

        let mut app = App::new(game);
        // Select the Reactors tab (index 2 in Tab::ALL) and the project.
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Reactors).unwrap();
        app.selected_item = 0;

        let backend = TestBackend::new(120, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, &app)).unwrap();

        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();

        assert!(text.contains("Reactors"), "pane title should render");
        assert!(text.contains("Mk1 Reactor"), "reactor name should render");
        assert!(text.contains("power loss"), "reactor-flavored flaw consequence should render");
        assert!(text.contains("Tech deficiencies"), "deficiency block should render");
        assert!(text.contains("Revise"), "[R] Revise control should render");
    }
}

#[cfg(test)]
mod market_discovery_render_tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// M2 Task 6 discovery rule: the Contracts pane shows only
    /// realized contracts and observed history. A campaign mission
    /// renders like any offered contract, and no seeded market
    /// internals (growth rates, presence draws, cadence/severity
    /// parameters) reach the buffer — which would only happen via an
    /// accidental debug-format of a Market or archetype.
    #[test]
    fn contracts_pane_shows_realized_only() {
        let mut game = crate::game_state::GameState::with_money("Render Test".into(), 100_000_000.0, 7);

        // Inject a live campaign mission alongside whatever the first
        // month generated.
        let campaign = crate::contract::Campaign {
            id: crate::contract::CampaignId(1),
            name: "Pathfinder Series".into(),
            market_id: crate::contract::MARKET_RIDESHARE,
            destination: "leo".into(),
            destination_display: "LEO".into(),
            payload_kg: 300.0,
            payment_per_mission: 4_050_000.0,
            missions_total: 4,
            missions_issued: 0,
            missions_missed: 0,
            next_issue_date: game.date,
            interval_days: 90,
            status: crate::contract::CampaignStatus::Won {
                by_player: true,
                company: "Render Test".into(),
            },
        };
        let mut rng = game.seed.world_query(crate::seed::WorldQuery::Test("render_test_campaign"));
        let mut next_id = crate::id::IdAllocator::<crate::contract::ContractId>::starting_at(900_000);
        let contract = crate::contract::campaign_contract(
            &campaign, (60, 150), &mut rng, &mut next_id, game.date,
        );
        game.available_contracts.push(contract);
        game.active_campaigns.push(campaign);

        let mut app = App::new(game);
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Contracts).unwrap();
        app.selected_item = 0;

        let backend = TestBackend::new(120, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, &app)).unwrap();

        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();

        // Realized content renders.
        assert!(text.contains("Rideshare"), "market group header should render");
        assert!(
            text.contains("Pathfinder Series Flight 1"),
            "campaign mission should render as an ordinary offered contract",
        );

        // Seeded internals never do. These strings only appear if a
        // Market/archetype is debug-printed into the UI.
        for leak in [
            "annual_growth", "presence_probability", "quiet_chance",
            "burst_chance", "failure_severity", "spawn_chance",
            "volume_mult", "base_volume", "trigger_year",
            // M3 award internals: the customer's budget and the
            // scoring weights are never shown.
            "budget_ceiling", "budget_tolerance", "rep_target",
            "w_cost", "w_rep", "rep_scale",
        ] {
            assert!(
                !text.contains(leak),
                "seeded parameter `{leak}` leaked into the contracts pane",
            );
        }
    }

    /// 'r' on the Contracts pane opens the standing bid-rules editor;
    /// Space toggles the selected market's rule, and the modal renders
    /// market names with their enable state and margin.
    #[test]
    fn r_opens_bid_rules_and_space_toggles() {
        let game = crate::game_state::GameState::with_money("Rules Test".into(), 100_000_000.0, 7);
        let first_active = game.markets.iter()
            .find(|m| m.active)
            .map(|m| (m.id, m.name.clone()))
            .expect("some market is active at start");
        let mut app = App::new(game);
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Contracts).unwrap();

        app.handle_key(KeyCode::Char('r'));
        assert!(
            matches!(app.input_mode, InputMode::BidRules { selected: 0 }),
            "'r' should open the bid-rules modal",
        );

        app.handle_key(KeyCode::Char(' '));
        let rule = app.game.player_company.bid_rules.get(&first_active.0)
            .expect("Space should create and enable the first market's rule");
        assert!(rule.enabled, "Space should toggle the rule on");

        let backend = TestBackend::new(120, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, &app)).unwrap();
        let text: String = terminal.backend().buffer()
            .content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Bid Rules"), "modal title should render");
        assert!(
            text.contains(&first_active.1),
            "active market `{}` should be listed in the rules modal", first_active.1,
        );
        assert!(text.contains("auto"), "the enabled rule should show as auto");
        assert!(text.contains("margin"), "margin column should render");

        app.handle_key(KeyCode::Esc);
        assert!(
            matches!(app.input_mode, InputMode::Normal),
            "Esc should close the modal",
        );
    }

    /// M3 Task 4 discovery rule: the award-history modal shows every
    /// observed outcome kind — and nothing the market never disclosed
    /// (no ceilings, no competitor cost/reliability internals).
    #[test]
    fn h_opens_award_history_showing_outcomes_only() {
        use crate::contract::{AwardOutcome, AwardRecord, MARKET_GEO_COMSATS};

        let mut game = crate::game_state::GameState::with_money("History Test".into(), 100_000_000.0, 7);
        let mk = |name: &str, outcome: AwardOutcome| AwardRecord {
            date: game.date,
            market_id: MARKET_GEO_COMSATS,
            contract_name: name.into(),
            destination: "gto".into(),
            payload_kg: 4_200.0,
            missions: None,
            outcome,
        };
        game.award_history.push(mk("WonSat", AwardOutcome::PlayerWon {
            amount: 88_000_000.0,
        }));
        game.award_history.push(mk("LostSat", AwardOutcome::CompetitorWon {
            company: "DinoSoar".into(),
            amount: 124_000_000.0,
            player_bid: Some(150_000_000.0),
        }));
        game.award_history.push(mk("HighSat", AwardOutcome::PlayerRejected {
            bid: 400_000_000.0,
        }));

        let mut app = App::new(game);
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Contracts).unwrap();
        app.handle_key(KeyCode::Char('h'));
        assert!(
            matches!(app.input_mode, InputMode::AwardHistory { scroll: 0 }),
            "'h' should open the award-history modal",
        );

        let backend = TestBackend::new(140, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, &app)).unwrap();
        let text: String = terminal.backend().buffer()
            .content().iter().map(|c| c.symbol()).collect();

        assert!(text.contains("Award History"), "modal title should render");
        assert!(text.contains("won at"), "player win should render with its price");
        assert!(text.contains("DinoSoar") && text.contains("(you"),
            "a lost award should show the winner's public price and the player's own bid");
        assert!(text.contains("over budget"),
            "a rejection should render without disclosing the ceiling");
        for leak in [
            "budget_ceiling", "budget_tolerance", "rep_target", "w_cost",
            "w_rep", "rep_scale", "failure_rate", "margin_min", "margin_max",
            "catalog_cost", "bid_floor",
        ] {
            assert!(
                !text.contains(leak),
                "hidden parameter `{leak}` leaked into the award-history modal",
            );
        }
    }

    /// A campaign block award renders in the history with its block
    /// size (the amount is per mission).
    #[test]
    fn award_history_tags_block_awards() {
        use crate::contract::{AwardOutcome, AwardRecord, MARKET_GEO_COMSATS};

        let mut game = crate::game_state::GameState::with_money("History Test".into(), 100_000_000.0, 7);
        game.award_history.push(AwardRecord {
            date: game.date,
            market_id: MARKET_GEO_COMSATS,
            contract_name: "Meridian Fleet".into(),
            destination: "gto".into(),
            payload_kg: 5_000.0,
            missions: Some(3),
            outcome: AwardOutcome::PlayerWon { amount: 88_000_000.0 },
        });
        let mut app = App::new(game);
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Contracts).unwrap();
        app.handle_key(KeyCode::Char('h'));

        let backend = TestBackend::new(140, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, &app)).unwrap();
        let text: String = terminal.backend().buffer()
            .content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("x3"),
            "a block award should carry its mission count",
        );
    }

    /// 'p' on the Contracts pane opens the programs modal: soliciting
    /// programs show only public facts (never the hidden reference or
    /// ceiling), Enter opens block-bid entry with a live block total,
    /// and a committed bid lands on the campaign and renders back.
    #[test]
    fn p_opens_programs_modal_bids_and_hides_internals() {
        use crate::contract::{Campaign, CampaignId, CampaignStatus, MARKET_GEO_COMSATS};

        let mut game = crate::game_state::GameState::with_money("Programs Test".into(), 100_000_000.0, 7);
        // Distinctive hidden numbers: if either renders anywhere the
        // leak assertions below trip.
        let mk = |id: u64, name: &str, status: CampaignStatus| Campaign {
            id: CampaignId(id),
            name: name.into(),
            market_id: MARKET_GEO_COMSATS,
            destination: "gto".into(),
            destination_display: "GTO".into(),
            payload_kg: 5_000.0,
            payment_per_mission: 137_930_000.0,
            missions_total: 3,
            missions_issued: 1,
            missions_missed: 0,
            next_issue_date: game.date,
            interval_days: 30,
            status,
        };
        game.active_campaigns.push(mk(1, "Sealed Program", CampaignStatus::Soliciting {
            bid_deadline: game.date.add_days(20),
            budget_ceiling_per_mission: 262_070_000.0,
            player_bid: None,
        }));
        game.active_campaigns.push(mk(2, "Rival Program", CampaignStatus::Won {
            by_player: false,
            company: "DinoSoar".into(),
        }));

        let mut app = App::new(game);
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Contracts).unwrap();
        app.handle_key(KeyCode::Char('p'));
        assert!(
            matches!(app.input_mode, InputMode::Campaigns { selected: 0 }),
            "'p' should open the programs modal",
        );

        let render = |app: &App| -> String {
            let backend = TestBackend::new(140, 50);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| draw::draw(frame, app)).unwrap();
            terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect()
        };
        let text = render(&app);
        assert!(text.contains("Programs"), "modal title should render");
        assert!(text.contains("Sealed Program") && text.contains("Rival Program"));
        assert!(text.contains("bids close"), "a soliciting program shows its deadline");
        assert!(
            text.contains("DinoSoar") && text.contains("1/3"),
            "a rival-held program shows its holder and flight progress",
        );
        // The rival's won price is public news; the soliciting
        // program's reference must NOT be visible even though it's
        // the same field.
        assert!(
            text.contains("137.9"),
            "the won program's public price should render",
        );
        assert_eq!(
            text.matches("137.9").count(), 1,
            "the soliciting program's hidden reference must not render",
        );
        assert!(
            !text.contains("262.1") && !text.contains("262,"),
            "the hidden ceiling must never render",
        );

        // Enter on the soliciting program → block-bid entry with a
        // live block total; Enter commits the sealed bid.
        app.handle_key(KeyCode::Enter);
        assert!(
            matches!(app.input_mode, InputMode::CampaignBidEntry { .. }),
            "Enter on a soliciting program should open block-bid entry",
        );
        for c in "85.5".chars() {
            app.handle_key(KeyCode::Char(c));
        }
        let text = render(&app);
        assert!(text.contains("per mission"), "entry is priced per mission");
        assert!(
            text.contains("Block total") && text.contains("256.5"),
            "the block total (bid x missions) should render live",
        );
        app.handle_key(KeyCode::Enter);
        assert!(
            matches!(app.input_mode, InputMode::Campaigns { .. }),
            "committing the bid returns to the programs list",
        );
        let bid = match &app.game.active_campaigns[0].status {
            CampaignStatus::Soliciting { player_bid, .. } => *player_bid,
            other => panic!("expected Soliciting, got {other:?}"),
        };
        assert_eq!(bid, Some(85_500_000.0), "the sealed bid should land on the campaign");
        let text = render(&app);
        assert!(text.contains("your bid"), "the player's own sealed bid renders back");
    }
}

#[cfg(test)]
mod engine_pane_tests {
    use super::*;
    use crate::engine_project::EngineDesignStatus;

    fn engines_app() -> App {
        let game = crate::game_state::GameState::with_money("Test".into(), 100_000_000.0, 1);
        let mut app = App::new(game);
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Engines).unwrap();
        app.selected_item = 0;
        app
    }

    /// Pressing 'n' in the Engines pane opens the standalone engine
    /// builder on a fresh Proposed engine; 'd' commits it to InDesign.
    #[test]
    fn n_opens_builder_and_d_commits() {
        let mut app = engines_app();
        let before = app.game.player_company.engine_projects.len();

        app.handle_key(KeyCode::Char('n'));
        assert_eq!(app.game.player_company.engine_projects.len(), before + 1,
            "a draft engine should be created");
        let pid = match &app.input_mode {
            InputMode::EngineEditor { project_id, state, .. } => {
                assert!(state.is_none(), "standalone editor should carry no designer state");
                *project_id
            }
            _ => panic!("expected the engine editor to open"),
        };
        // The draft is Proposed until committed.
        let ep = app.game.player_company.find_engine_project(pid).unwrap();
        assert!(matches!(ep.status, EngineDesignStatus::Proposed { .. }));

        // 'd' commits: Proposed → InDesign, modal closes.
        app.handle_key(KeyCode::Char('d'));
        assert!(matches!(app.input_mode, InputMode::Normal), "editor should close on commit");
        let ep = app.game.player_company.find_engine_project(pid).unwrap();
        assert!(matches!(ep.status, EngineDesignStatus::InDesign { .. }),
            "committed engine should be InDesign");
    }

    /// Esc from the standalone builder discards the draft entirely.
    #[test]
    fn esc_discards_the_draft() {
        let mut app = engines_app();
        let before = app.game.player_company.engine_projects.len();

        app.handle_key(KeyCode::Char('n'));
        assert_eq!(app.game.player_company.engine_projects.len(), before + 1);

        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.input_mode, InputMode::Normal));
        assert_eq!(app.game.player_company.engine_projects.len(), before,
            "cancelling should delete the Proposed draft");
    }
}

#[cfg(test)]
mod contract_table_render_tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// Character (not byte) column at which `needle` starts. `▶` and
    /// `…` are multi-byte, so `str::find` offsets don't line up with
    /// screen columns.
    fn col_of(line: &str, needle: &str) -> Option<usize> {
        line.find(needle).map(|b| line[..b].chars().count())
    }

    /// Render the app and return the buffer as one string per row.
    fn render_lines(app: &App, w: u16, h: u16) -> (Vec<String>, ratatui::buffer::Buffer) {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let lines = (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect();
        (lines, buf)
    }

    /// Contracts whose names differ wildly in length — the case that
    /// used to shear every column to the right of the name.
    fn contracts_with_varied_names() -> Vec<crate::contract::Contract> {
        let names = ["Sat-1", "Meridian Heavy Communications Platform", "Orbcomm 7"];
        names.iter().enumerate().map(|(i, name)| crate::contract::Contract {
            id: crate::contract::ContractId(100 + i as u64),
            name: (*name).into(),
            destination: "leo".into(),
            payload_kg: 450.0 + i as f64 * 1200.0,
            payment: 12_500_000.0 + i as f64 * 3_000_000.0,
            deadline: crate::calendar::GameDate::new(2001, 9, 1),
            status: crate::contract::ContractStatus::Available,
            market_id: crate::contract::MARKET_RIDESHARE,
            campaign_id: None,
            bid_deadline: None,
            budget_ceiling: 0.0,
            player_bid: None,
        }).collect()
    }

    fn app_with_contracts() -> App {
        let mut game = crate::game_state::GameState::with_money("Column Test".into(), 100_000_000.0, 7);
        game.available_contracts = contracts_with_varied_names();
        let mut app = App::new(game);
        app.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Contracts).unwrap();
        app.selected_item = 0;
        app
    }

    /// M5 Task 0: every contract row puts its columns at the same
    /// offsets regardless of how long the contract name is.
    #[test]
    fn contract_columns_align_across_rows() {
        let app = app_with_contracts();
        let (lines, _) = render_lines(&app, 120, 50);

        let rows: Vec<&String> = lines.iter()
            .filter(|l| l.contains(" kg ") && l.contains("Sep 1, 2001"))
            .collect();
        assert_eq!(rows.len(), 3, "all three contracts should render as rows");

        let kg_at: Vec<Option<usize>> = rows.iter().map(|l| col_of(l, " kg")).collect();
        assert!(
            kg_at.windows(2).all(|w| w[0] == w[1]),
            "payload column must start at one offset, got {kg_at:?}\n{rows:#?}",
        );
        let date_at: Vec<Option<usize>> = rows.iter()
            .map(|l| col_of(l, "Sep 1, 2001")).collect();
        assert!(
            date_at.windows(2).all(|w| w[0] == w[1]),
            "deadline column must start at one offset, got {date_at:?}",
        );

        // The long name is elided rather than allowed to push the row.
        let (narrow, _) = render_lines(&app, 90, 50);
        assert!(
            narrow.iter().any(|l| l.contains('…')),
            "an over-long contract name should be truncated with an ellipsis",
        );
    }

    /// M5 Task 0: selection is marked with a background, so it can't
    /// collide with the yellow that means "borderline / needs build".
    /// Before this change both used `fg(Yellow)` and were identical.
    #[test]
    fn selection_uses_background_not_a_foreground_colour() {
        let app = app_with_contracts();
        let (lines, buf) = render_lines(&app, 120, 50);

        let y = lines.iter().position(|l| l.contains("▶ ")).expect("a selected row renders");
        let x = col_of(&lines[y], "▶").expect("marker present") as u16;
        let cell = &buf[(x, y as u16)];
        assert_eq!(
            cell.bg, draw::SELECTION_BG,
            "the selected row should carry the selection background",
        );

        // An unselected row must not, or the signal means nothing.
        let other = lines.iter().enumerate()
            .find(|(i, l)| *i != y && l.contains(" kg ") && l.contains("Sep 1, 2001"))
            .map(|(i, _)| i)
            .expect("a second contract row renders");
        assert_ne!(
            buf[(x, other as u16)].bg, draw::SELECTION_BG,
            "unselected rows must not carry the selection background",
        );
    }

    /// The table degrades to fewer columns rather than overflowing when
    /// the pane is narrow (80-column terminals are still a thing).
    #[test]
    fn narrow_pane_drops_the_bid_close_column_instead_of_overflowing() {
        let app = app_with_contracts();
        let (wide, _) = render_lines(&app, 140, 50);
        let (narrow, _) = render_lines(&app, 80, 50);

        assert!(
            wide.iter().any(|l| l.contains("Bids close")),
            "a wide pane shows the bid-close column",
        );
        assert!(
            !narrow.iter().any(|l| l.contains("Bids close")),
            "a narrow pane drops it rather than running past the border",
        );
        // Nothing spills past the right border on either width.
        for l in narrow.iter().filter(|l| l.contains("Sep 1, 2001")) {
            assert!(l.ends_with('│') || l.trim_end().chars().count() <= 79,
                "row overflows the 80-column pane: {l:?}");
        }
    }
}

#[cfg(test)]
mod ready_rocket_order_tests {
    use crate::company::Company;
    use crate::manufacturing::{InventoryItemId, InventoryRocket};
    use crate::rocket::RocketDesignId;
    use crate::rocket_project::RocketProjectId;

    fn test_company() -> Company {
        Company::new(
            "Test".into(), 0.0,
            &crate::seed::GameSeed::new(1),
            &crate::balance_config::BalanceConfig::default(),
        )
    }

    fn stock(company: &mut Company, item_id: u64, name: &str, revision: u32) {
        company.manufacturing.inventory.rockets.push(InventoryRocket {
            item_id: InventoryItemId(item_id),
            rocket_project_id: RocketProjectId(1),
            design_id: RocketDesignId(1),
            rocket_name: name.into(),
            build_cost: 1_000_000.0,
            revision,
            rocket_flaws: Vec::new(),
        });
    }

    #[test]
    fn ready_rockets_are_grouped_by_name_newest_revision_first() {
        let mut company = test_company();
        // Pushed in build order, which is nobody's idea of a useful list.
        stock(&mut company, 1, "Zephyr", 0);
        stock(&mut company, 2, "Atlas", 2);
        stock(&mut company, 3, "Zephyr", 1);
        stock(&mut company, 4, "Atlas", 5);
        stock(&mut company, 5, "Atlas", 2);

        let ordered: Vec<_> = company.ready_rockets_in_display_order().iter()
            .map(|r| (r.rocket_name.as_str(), r.revision, r.item_id.0))
            .collect();
        assert_eq!(ordered, vec![
            ("Atlas", 5, 4),
            ("Atlas", 2, 2),  // equal revision falls back to build order,
            ("Atlas", 2, 5),  // so the list never reshuffles under the cursor
            ("Zephyr", 1, 3),
            ("Zephyr", 0, 1),
        ]);
    }

    #[test]
    fn display_order_is_a_permutation_of_the_inventory() {
        // The Launches tab clamps the cursor against the raw inventory
        // length, so the reordered view has to be the same size and hold
        // the same items.
        let mut company = test_company();
        stock(&mut company, 1, "Zephyr", 0);
        stock(&mut company, 2, "Atlas", 2);
        stock(&mut company, 3, "Zephyr", 1);

        let ordered = company.ready_rockets_in_display_order();
        assert_eq!(ordered.len(), company.manufacturing.inventory.rockets.len());
        let mut seen: Vec<u64> = ordered.iter().map(|r| r.item_id.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2, 3]);
    }
}

/// The retire confirmation: `X` only asks, `Esc` backs out, `Y`
/// commits. The whole point of the prompt is that the first two
/// change nothing.
#[cfg(test)]
mod retire_confirmation_tests {
    use super::*;
    use crate::ui::modals::help_tests::{app, render};

    fn rockets_app() -> App {
        let mut a = app();
        a.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Rockets).unwrap();
        a.focused_pane = FocusedPane::Content;
        a.selected_item = 0;
        a
    }

    #[test]
    fn x_only_asks() {
        let mut a = rockets_app();
        let before = a.game.player_company.visible_rocket_projects().count();
        assert!(before > 0, "fixture should have a rocket to retire");

        a.handle_key(KeyCode::Char('x'));

        assert!(matches!(a.input_mode, InputMode::ConfirmRetire { .. }),
            "X should open the prompt, got {:?}", a.input_mode);
        assert_eq!(a.game.player_company.visible_rocket_projects().count(), before,
            "asking must not retire anything");
    }

    #[test]
    fn esc_backs_out_without_retiring() {
        let mut a = rockets_app();
        let before = a.game.player_company.visible_rocket_projects().count();
        a.handle_key(KeyCode::Char('x'));
        a.handle_key(KeyCode::Esc);

        assert!(matches!(a.input_mode, InputMode::Normal));
        assert_eq!(a.game.player_company.visible_rocket_projects().count(), before);
        assert!(a.game.player_company.rocket_projects.iter().all(|rp| !rp.retired));
    }

    /// Enter confirms in half the other modals, so it deliberately
    /// does *not* here — muscle memory shouldn't retire a design.
    #[test]
    fn enter_does_not_confirm() {
        let mut a = rockets_app();
        a.handle_key(KeyCode::Char('x'));
        a.handle_key(KeyCode::Enter);

        assert!(matches!(a.input_mode, InputMode::Normal));
        assert!(a.game.player_company.rocket_projects.iter().all(|rp| !rp.retired),
            "Enter must not be a confirm here");
    }

    #[test]
    fn y_retires_and_the_row_disappears() {
        let mut a = rockets_app();
        let before = a.game.player_company.visible_rocket_projects().count();
        a.handle_key(KeyCode::Char('x'));
        a.handle_key(KeyCode::Char('y'));

        assert!(matches!(a.input_mode, InputMode::Normal));
        assert_eq!(a.game.player_company.visible_rocket_projects().count(), before - 1);
        assert!(a.game.player_company.rocket_projects.iter().any(|rp| rp.retired));
    }

    /// The prompt holds an id, not a row. A day ticking past while
    /// it is open must not redirect it at a different design.
    #[test]
    fn the_prompt_acts_on_the_design_it_named() {
        let mut a = rockets_app();
        let target = a.game.player_company.rocket_projects[0].project_id;
        a.handle_key(KeyCode::Char('x'));

        // Something else lands at the top of the list underneath the
        // open prompt.
        let mut other = a.game.player_company.rocket_projects[0].clone();
        other.project_id = crate::rocket_project::RocketProjectId(9999);
        other.design.name = "Interloper".into();
        a.game.player_company.rocket_projects.insert(0, other);

        a.handle_key(KeyCode::Char('y'));

        let retired: Vec<_> = a.game.player_company.rocket_projects.iter()
            .filter(|rp| rp.retired).map(|rp| rp.project_id).collect();
        assert_eq!(retired, vec![target],
            "the prompt should retire what it named, not whatever row moved into place");
    }

    /// A refusal explains itself in the status line rather than
    /// opening a prompt that would lie about what it is going to do.
    #[test]
    fn a_blocked_engine_explains_itself_instead_of_asking() {
        let mut a = app();
        a.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Engines).unwrap();
        a.focused_pane = FocusedPane::Content;
        a.selected_item = 0;

        a.handle_key(KeyCode::Char('x'));

        assert!(matches!(a.input_mode, InputMode::Normal),
            "a refusal shouldn't open the prompt, got {:?}", a.input_mode);
        let msg = a.status_message.clone().unwrap_or_default();
        assert!(msg.contains("Fixture-1"),
            "the message should name the rocket that blocks it, got {msg:?}");
        assert!(a.game.player_company.engine_projects.iter().all(|ep| !ep.retired));
    }

    /// The modal has to say what it is about to destroy.
    #[test]
    fn the_prompt_spells_out_the_consequences() {
        let mut a = rockets_app();
        let id = a.game.player_company.rocket_projects[0].project_id;
        // Auto-build targets only attach to a Testing design.
        a.game.player_company.rocket_projects[0].status =
            RocketDesignStatus::Testing { work_completed: 100.0 };
        assert!(a.game.player_company.set_auto_build_target(id, 2));
        a.handle_key(KeyCode::Char('x'));

        let screen = render(&a, 120, 44);
        assert!(screen.contains("Retire Fixture-1?"),
            "the prompt should name the design:\n{screen}");
        assert!(screen.contains("[Y] Retire"), "it should show the confirm key");
        assert!(screen.contains("Esc"), "it should show the way out");
        assert!(screen.contains("Auto-build will be switched off"),
            "a standing auto-build target is a consequence worth naming:\n{screen}");
    }

    /// Retiring the top row of several shouldn't move the cursor;
    /// retiring the last row has to pull it back inside the list.
    #[test]
    fn the_cursor_stays_inside_the_shortened_list() {
        let mut a = rockets_app();
        // Two more designs so there is somewhere to sit.
        for i in 0..2 {
            let mut extra = a.game.player_company.rocket_projects[0].clone();
            extra.project_id = crate::rocket_project::RocketProjectId(500 + i);
            a.game.player_company.rocket_projects.push(extra);
        }
        let count = a.game.player_company.visible_rocket_projects().count();
        assert_eq!(count, 3);

        a.selected_item = 0;
        a.handle_key(KeyCode::Char('x'));
        a.handle_key(KeyCode::Char('y'));
        assert_eq!(a.selected_item, 0, "retiring the top row shouldn't move the cursor");

        a.selected_item = 1;
        a.handle_key(KeyCode::Char('x'));
        a.handle_key(KeyCode::Char('y'));
        assert_eq!(a.selected_item, 0,
            "the cursor must not be left past the end of the list");
        assert!(a.selected_project(ProjectKind::Rocket).is_some(),
            "and it should still resolve to a real project");
    }
}
