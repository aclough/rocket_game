//! `handle_input_mode_key`: one arm per modal, from the help screen to
//! the delta-v planner. The single biggest match in the UI.

use super::*;
use super::planner::reachable_destinations_multistage;
use super::text_field::{edit_text_field, FieldEdit, FieldKind};

/// A bid typed in $M, as dollars; `None` unless it is a positive number.
fn parse_bid_millions(buffer: &str) -> Option<f64> {
    match buffer.trim().parse::<f64>() {
        Ok(m) if m > 0.0 => Some(m * 1_000_000.0),
        _ => None,
    }
}

impl App {
    pub(super) fn handle_input_mode_key(&mut self, key: KeyCode) {
        match &mut self.input_mode {
            InputMode::Normal => unreachable!(),
            InputMode::Intro => {
                // Any key dismisses.
                self.exit_modal();
            }
            InputMode::ConfirmRetire { target, .. } => {
                // Only `Y` retires. Deliberately no Enter default —
                // Enter confirms in half the other modals, and muscle
                // memory shouldn't be able to retire a design.
                let target = *target;
                if !matches!(key, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    self.exit_modal();
                    self.status_message = Some("Cancelled".into());
                    return;
                }
                self.exit_modal();
                // Re-planned inside `retire`, so a day tick that landed
                // while the prompt was open is accounted for rather than
                // acted on from a stale snapshot.
                match self.game.player_company.retire(target) {
                    Ok(e) => {
                        let mut parts = vec![format!("Retired {}", e.design_name)];
                        if e.teams_released > 0 {
                            parts.push(format!("{} team(s) freed", e.teams_released));
                        }
                        if !e.cancelled.is_empty() {
                            parts.push(format!("{} order(s) cancelled", e.cancelled.len()));
                        }
                        if !e.reassigned_engine_orders.is_empty() {
                            parts.push(format!(
                                "{} engine build(s) kept", e.reassigned_engine_orders.len()));
                        }
                        self.status_message = Some(parts.join(" — "));
                        // The row is gone. Only pull the cursor back if
                        // it now points past the end — retiring row 0 of
                        // three shouldn't move the selection at all.
                        let remaining = self.game.player_company
                            .visible_projects(target.kind()).count();
                        clamp_cursor(&mut self.selected_item, remaining);
                    }
                    Err(_) => {
                        self.status_message = Some("Could not retire that design".into());
                    }
                }
            }
            InputMode::Help { .. } => {
                // Any key dismisses.
                self.exit_modal();
            }
            InputMode::ReactorEditor { .. }
            | InputMode::ReactorEditorField { .. } => {
                let old_mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
                match old_mode {
                    InputMode::ReactorEditor { project_id, cursor } => {
                        self.handle_reactor_editor_key(key, project_id, cursor);
                    }
                    InputMode::ReactorEditorField { project_id, cursor, field, mut buffer } => {
                        match edit_text_field(key, &mut buffer, field.kind()) {
                            FieldEdit::Continue => {
                                self.input_mode = InputMode::ReactorEditorField {
                                    project_id, cursor, field, buffer,
                                };
                            }
                            FieldEdit::Commit => {
                                self.commit_reactor_field(project_id, field, &buffer);
                                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
                            }
                            FieldEdit::Cancel => {
                                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            InputMode::EngineEditor { .. }
            | InputMode::EngineEditorField { .. } => {
                // Extract and dispatch separately to avoid holding a
                // mutable borrow on self.input_mode while calling self
                // methods.
                let old_mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
                match old_mode {
                    InputMode::EngineEditor { project_id, cursor, state } => {
                        self.handle_engine_editor_key(key, project_id, cursor, state);
                    }
                    InputMode::EngineEditorField { project_id, cursor, field, mut buffer, mut state } => {
                        match edit_text_field(key, &mut buffer, field.kind()) {
                            FieldEdit::Continue => {
                                self.input_mode = InputMode::EngineEditorField {
                                    project_id, cursor, field, buffer, state,
                                };
                            }
                            FieldEdit::Commit => {
                                self.commit_engine_field(project_id, field, &buffer, &mut state);
                                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
                            }
                            FieldEdit::Cancel => {
                                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            InputMode::SelectThirdParty { selected } => {
                let catalog_len = self.game.player_company.third_party_catalog.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(selected),
                    KeyCode::Down => cursor_down(selected, catalog_len),
                    KeyCode::Enter => {
                        let idx = *selected;
                        let date = self.game.date;
                        self.exit_modal();
                        let seed_clone = self.game.seed.clone();
                        if let Some(evt) = self.game.player_company.contract_third_party(idx, date, &seed_clone, &self.game.balance) {
                            self.game.log(evt);
                            self.status_message = Some("Engine contracted".into());
                        }
                    }
                    _ => {}
                }
            }
            InputMode::BidEntry { contract_index, buffer } => {
                let index = *contract_index;
                match edit_text_field(key, buffer, FieldKind::Number) {
                    FieldEdit::Continue => {}
                    FieldEdit::Cancel => { self.exit_modal(); }
                    FieldEdit::Commit => {
                        let bid = parse_bid_millions(buffer);
                        self.exit_modal();
                        match bid {
                            Some(bid) => {
                                if let Some(evt) = self.game.place_bid(index, bid) {
                                    self.status_message = Some(format!("{}", evt));
                                } else {
                                    self.status_message = Some("Could not place bid".into());
                                }
                            }
                            None => {
                                self.status_message = Some("Bid must be a positive number of $M".into());
                            }
                        }
                    }
                }
            }
            InputMode::BidRules { selected } => {
                let market_ids: Vec<crate::contract::MarketId> = self.game.markets.iter()
                    .filter(|m| m.active)
                    .map(|m| m.id)
                    .collect();
                match key {
                    KeyCode::Esc | KeyCode::Char('r') | KeyCode::Char('R') => {
                        self.exit_modal();
                    }
                    KeyCode::Up | KeyCode::Char('k') => cursor_up(selected),
                    KeyCode::Down | KeyCode::Char('j') => cursor_down(selected, market_ids.len()),
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        if let Some(&id) = market_ids.get(*selected) {
                            let rule = self.game.player_company.bid_rules
                                .entry(id).or_default();
                            rule.enabled = !rule.enabled;
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if let Some(&id) = market_ids.get(*selected) {
                            let rule = self.game.player_company.bid_rules
                                .entry(id).or_default();
                            rule.margin = (rule.margin - 0.25).max(0.0);
                        }
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        if let Some(&id) = market_ids.get(*selected) {
                            let rule = self.game.player_company.bid_rules
                                .entry(id).or_default();
                            rule.margin = (rule.margin + 0.25).min(9.0);
                        }
                    }
                    _ => {}
                }
            }
            InputMode::AwardHistory { scroll } => {
                let len = self.game.award_history.len();
                match key {
                    KeyCode::Esc | KeyCode::Char('h') | KeyCode::Char('H') => {
                        self.exit_modal();
                    }
                    KeyCode::Up | KeyCode::Char('k') => cursor_up(scroll),
                    KeyCode::Down | KeyCode::Char('j') => cursor_down(scroll, len),
                    _ => {}
                }
            }
            InputMode::Campaigns { selected } => {
                let len = self.game.active_campaigns.len();
                match key {
                    KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char('P') => {
                        self.exit_modal();
                    }
                    KeyCode::Up | KeyCode::Char('k') => cursor_up(selected),
                    KeyCode::Down | KeyCode::Char('j') => cursor_down(selected, len),
                    KeyCode::Enter | KeyCode::Char('b') | KeyCode::Char('B') => {
                        let sel = *selected;
                        let Some(c) = self.game.active_campaigns.get(sel) else {
                            return;
                        };
                        match c.status {
                            crate::contract::CampaignStatus::Soliciting { player_bid, .. } => {
                                // Seed the buffer with any pending bid
                                // so it can be revised.
                                let buffer = player_bid
                                    .map(|b| format!("{}", b / 1_000_000.0))
                                    .unwrap_or_default();
                                let id = c.id;
                                self.input_mode = InputMode::CampaignBidEntry {
                                    campaign_id: id,
                                    selected: sel,
                                    buffer,
                                };
                            }
                            crate::contract::CampaignStatus::Won { .. } => {
                                self.status_message =
                                    Some("Program already awarded".into());
                            }
                        }
                    }
                    _ => {}
                }
            }
            InputMode::CampaignBidEntry { campaign_id, selected, buffer } => {
                let id = *campaign_id;
                let back = *selected;
                match edit_text_field(key, buffer, FieldKind::Number) {
                    FieldEdit::Continue => {}
                    FieldEdit::Cancel => {
                        self.input_mode = InputMode::Campaigns { selected: back };
                    }
                    FieldEdit::Commit => {
                        match parse_bid_millions(buffer) {
                            Some(bid) => {
                                if let Some(evt) = self.game.place_campaign_bid(id, bid) {
                                    self.status_message = Some(format!("{}", evt));
                                } else {
                                    self.status_message = Some("Could not place block bid".into());
                                }
                            }
                            None => {
                                self.status_message =
                                    Some("Bid must be a positive number of $M per mission".into());
                            }
                        }
                        self.input_mode = InputMode::Campaigns { selected: back };
                    }
                }
            }
            InputMode::RocketName { buffer } => {
                match edit_text_field(key, buffer, FieldKind::Text) {
                    FieldEdit::Continue => {}
                    FieldEdit::Cancel => { self.exit_modal(); }
                    FieldEdit::Commit => {
                        if buffer.is_empty() {
                            self.status_message = Some("Name cannot be empty".into());
                            self.exit_modal();
                        } else {
                            let name = buffer.clone();
                            self.input_mode = InputMode::designer(
                                Box::new(RocketDesignerState::new(name)),
                            );
                        }
                    }
                }
            }
            InputMode::RocketDesigner { .. } => {
                // Take the state out before calling handlers, so none
                // holds a borrow on self.input_mode; each puts it back.
                let old_mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
                let InputMode::RocketDesigner { state, sub } = old_mode else { unreachable!() };
                match sub {
                    DesignerSubMode::Main => self.handle_rocket_designer_key(key, state),
                    DesignerSubMode::PickEngine(pick) => {
                        self.handle_rocket_pick_engine_key(key, state, pick);
                    }
                    DesignerSubMode::PayloadInput { buffer } => {
                        self.handle_rocket_payload_input_key(key, state, buffer);
                    }
                    DesignerSubMode::LocationPicker { target, locations, selected } => {
                        self.handle_rocket_designer_location_picker_key(
                            key, state, target, locations, selected,
                        );
                    }
                    DesignerSubMode::PowerEditor { group_index, stage_index, cursor } => {
                        self.handle_power_editor_key(key, state, group_index, stage_index, cursor);
                    }
                    // Any key closes help and returns to the design in
                    // progress.
                    DesignerSubMode::Help => self.input_mode = InputMode::designer(state),
                }
            }
            InputMode::LaunchManifest {
                rocket_item_id, persist, contract_picks, spacecraft_picks,
                spacecraft_item_ids, cursor,
            } => {
                let rocket_item_id = *rocket_item_id;
                let persist = *persist;
                let num_contracts = contract_picks.len();
                let num_spacecraft = spacecraft_picks.len();
                let total_rows = num_contracts + num_spacecraft;
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(cursor),
                    KeyCode::Down => cursor_down(cursor, total_rows),
                    KeyCode::Char(' ') => {
                        if *cursor < num_contracts {
                            contract_picks[*cursor] = !contract_picks[*cursor];
                        } else if *cursor - num_contracts < num_spacecraft {
                            let idx = *cursor - num_contracts;
                            spacecraft_picks[idx] = !spacecraft_picks[idx];
                        }
                    }
                    KeyCode::Enter => {
                        // Snapshot picks (we'll need to mutate game state).
                        let contract_picks = contract_picks.clone();
                        let spacecraft_picks = spacecraft_picks.clone();
                        let spacecraft_item_ids = spacecraft_item_ids.clone();
                        self.submit_manifest_launch(
                            rocket_item_id, persist,
                            contract_picks, spacecraft_picks, spacecraft_item_ids,
                        );
                    }
                    _ => {}
                }
            }
            InputMode::LaunchResult { .. } => {
                // Any key dismisses the result
                match key {
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char(_) => {
                        self.exit_modal();
                    }
                    _ => {}
                }
            }
            InputMode::FlySelectSpacecraft { selected } => {
                let num_spacecraft = self.game.spacecraft.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(selected),
                    KeyCode::Down => cursor_down(selected, num_spacecraft),
                    KeyCode::Enter => {
                        let selected = *selected;
                        let sc = &self.game.spacecraft[selected];
                        let remaining_dv = sc.remaining_delta_v();
                        // Use the live sum of carried payload masses rather than
                        // the cached `payload_mass_kg`, which may be stale if
                        // payloads were detached on a previous flight.
                        let payload_mass: f64 = sc.payloads.iter().map(|p| p.mass_kg()).sum();
                        let rocket_mass = payload_mass + sc.design.total_mass_kg();
                        let destinations = reachable_destinations_multistage(
                            sc.location.name(), remaining_dv, rocket_mass,
                            Some(&sc.rocket), Some(&sc.design),
                        );
                        if destinations.is_empty() {
                            self.status_message = Some("No reachable destinations for this spacecraft".into());
                            return;
                        }
                        self.input_mode = InputMode::FlySelectDestination {
                            spacecraft_index: selected,
                            destinations,
                            remaining_dv,
                            selected: 0,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::FlySelectDestination { spacecraft_index, destinations, selected, .. } => {
                let spacecraft_index = *spacecraft_index;
                let num_destinations = destinations.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(selected),
                    KeyCode::Down => cursor_down(selected, num_destinations),
                    KeyCode::Enter => {
                        let selected = *selected;
                        if let InputMode::FlySelectDestination { destinations, .. } = &self.input_mode {
                            let dest_id = destinations[selected].0.clone();
                            self.game.fly_spacecraft(spacecraft_index, &dest_id);
                            self.status_message = Some("Spacecraft flight departed".into());
                            self.exit_modal();
                        }
                    }
                    _ => {}
                }
            }
            InputMode::DockSelectSmall { selected } => {
                let num = self.game.spacecraft.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(selected),
                    KeyCode::Down => cursor_down(selected, num),
                    KeyCode::Enter => {
                        let selected = *selected;
                        // Build candidate list: other spacecraft at the
                        // same location as the chosen "small" one.
                        let small_loc = &self.game.spacecraft[selected].location;
                        let candidates: Vec<usize> = self.game.spacecraft.iter().enumerate()
                            .filter(|(i, sc)| *i != selected && sc.location == *small_loc)
                            .map(|(i, _)| i)
                            .collect();
                        if candidates.is_empty() {
                            self.status_message = Some(
                                "No other spacecraft at this location".into());
                            self.exit_modal();
                            return;
                        }
                        self.input_mode = InputMode::DockSelectLarge {
                            small_idx: selected, candidates, selected: 0,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::DockSelectLarge { small_idx, candidates, selected } => {
                let small_idx = *small_idx;
                let num = candidates.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(selected),
                    KeyCode::Down => cursor_down(selected, num),
                    KeyCode::Enter => {
                        let selected = *selected;
                        let large_idx = candidates[selected];
                        if self.game.dock_spacecraft(small_idx, large_idx) {
                            self.status_message = Some("Docked".into());
                        } else {
                            self.status_message = Some("Dock failed".into());
                        }
                        self.exit_modal();
                    }
                    _ => {}
                }
            }
            InputMode::UndockSelectCarrier { candidates, selected } => {
                let num = candidates.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(selected),
                    KeyCode::Down => cursor_down(selected, num),
                    KeyCode::Enter => {
                        let selected = *selected;
                        let carrier_idx = candidates[selected];
                        let payload_indices: Vec<usize> = self.game.spacecraft[carrier_idx]
                            .payloads.iter().enumerate()
                            .filter(|(_, p)| matches!(p, crate::flight::Payload::Spacecraft { .. }))
                            .map(|(i, _)| i)
                            .collect();
                        if payload_indices.is_empty() {
                            self.status_message = Some("No spacecraft payloads".into());
                            self.exit_modal();
                            return;
                        }
                        self.input_mode = InputMode::UndockSelectPayload {
                            carrier_idx, payload_indices, selected: 0,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::UndockSelectPayload { carrier_idx, payload_indices, selected } => {
                let carrier_idx = *carrier_idx;
                let num = payload_indices.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(selected),
                    KeyCode::Down => cursor_down(selected, num),
                    KeyCode::Enter => {
                        let selected = *selected;
                        let payload_idx = payload_indices[selected];
                        if self.game.undock_payload(carrier_idx, payload_idx) {
                            self.status_message = Some("Undocked".into());
                        } else {
                            self.status_message = Some("Undock failed".into());
                        }
                        self.exit_modal();
                    }
                    _ => {}
                }
            }
            InputMode::PlannerSetup { state } => {
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Tab => {
                        state.active_field = match state.active_field {
                            PlannerSetupField::Design => PlannerSetupField::Payload,
                            PlannerSetupField::Payload => PlannerSetupField::Location,
                            PlannerSetupField::Location => PlannerSetupField::Design,
                        };
                    }
                    KeyCode::Up => {
                        match state.active_field {
                            PlannerSetupField::Design => cursor_up(&mut state.selected_project),
                            PlannerSetupField::Location => cursor_up(&mut state.selected_location),
                            PlannerSetupField::Payload => {}
                        }
                    }
                    KeyCode::Down => {
                        match state.active_field {
                            PlannerSetupField::Design => {
                                cursor_down(&mut state.selected_project, state.eligible_projects.len());
                            }
                            PlannerSetupField::Location => {
                                cursor_down(&mut state.selected_location, state.locations.len());
                            }
                            PlannerSetupField::Payload => {}
                        }
                    }
                    KeyCode::Char(c) if state.active_field == PlannerSetupField::Payload => {
                        if c.is_ascii_digit() || c == '.' {
                            state.payload_buffer.push(c);
                        }
                    }
                    KeyCode::Backspace if state.active_field == PlannerSetupField::Payload => {
                        state.payload_buffer.pop();
                    }
                    KeyCode::Enter => {
                        // Launch the planner with chosen parameters
                        let pi = state.eligible_projects[state.selected_project];
                        let rp = &self.game.player_company.rocket_projects[pi];
                        let payload_kg: f64 = state.payload_buffer.parse().unwrap_or(0.0);
                        let (start_id, _) = state.locations[state.selected_location];
                        let rocket = rp.design.instantiate(
                            crate::rocket::RocketId(0), payload_kg,
                        );
                        let remaining_dv = rocket.remaining_delta_v(&rp.design);
                        let rocket_mass = rp.design.total_mass_kg() + payload_kg;
                        let destinations = reachable_destinations_multistage(
                            start_id, remaining_dv, rocket_mass,
                            Some(&rocket), Some(&rp.design),
                        );
                        self.input_mode = InputMode::DvPlanner {
                            state: Box::new(DvPlannerState {
                                source: PlannerSource::Design { project_index: pi },
                                rocket,
                                design: rp.design.clone(),
                                current_location: crate::location::LocationId::of(start_id),
                                actions: vec![],
                                snapshots: vec![],
                                destinations,
                                selected: 0,
                                payload_kg,
                            }),
                        };
                    }
                    _ => {}
                }
            }
            InputMode::DvPlanner { state } => {
                let num_dests = state.destinations.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => cursor_up(&mut state.selected),
                    KeyCode::Down => cursor_down(&mut state.selected, num_dests),
                    KeyCode::Enter => {
                        // Select destination — simulate the burn
                        if state.selected < num_dests {
                            let (dest_id, dest_display, dv_cost) =
                                state.destinations[state.selected].clone();
                            let from = state.current_location;

                            // Save snapshot for undo
                            state.snapshots.push((state.rocket.clone(), state.payload_kg, from));

                            // Burn propellant the way the flight will: the
                            // transfer plus, leaving a surface, the ascent's
                            // gravity loss; the Isp penalty rides on the burn.
                            let leg_cost = dv_cost
                                + crate::flight::ascent_gravity_cost(&state.design, state.payload_kg, from.name());
                            let _ = state.rocket.burn_sequential(&state.design, leg_cost, from.name());
                            state.current_location = crate::location::LocationId::of(&dest_id);

                            state.actions.push(PlanAction::Leg {
                                from,
                                to: state.current_location,
                                to_display: dest_display,
                                dv_cost: leg_cost,
                            });

                            // Recompute destinations
                            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
                            let rocket_mass = state.design.total_mass_kg() + state.payload_kg;
                            state.destinations = reachable_destinations_multistage(
                                state.current_location.name(), remaining_dv, rocket_mass,
                                Some(&state.rocket), Some(&state.design),
                            );
                            state.selected = 0;
                        }
                    }
                    KeyCode::Char('d') => {
                        // Drop payload
                        if state.payload_kg > 0.0 {
                            let mass = state.payload_kg;
                            state.snapshots.push((state.rocket.clone(), state.payload_kg, state.current_location));
                            state.payload_kg = 0.0;
                            state.rocket.payload_mass_kg = 0.0;
                            state.actions.push(PlanAction::DropPayload { mass_dropped: mass });

                            // Recompute destinations with new mass
                            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
                            let rocket_mass = state.design.total_mass_kg();
                            state.destinations = reachable_destinations_multistage(
                                state.current_location.name(), remaining_dv, rocket_mass,
                                Some(&state.rocket), Some(&state.design),
                            );
                            clamp_cursor(&mut state.selected, state.destinations.len());
                        }
                    }
                    KeyCode::Char('u') => {
                        // Undo last action
                        if let Some((prev_rocket, prev_payload, prev_location)) = state.snapshots.pop() {
                            state.actions.pop();
                            state.rocket = prev_rocket;
                            state.payload_kg = prev_payload;
                            state.rocket.payload_mass_kg = prev_payload;
                            state.current_location = prev_location;

                            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
                            let rocket_mass = state.design.total_mass_kg() + state.payload_kg;
                            state.destinations = reachable_destinations_multistage(
                                state.current_location.name(), remaining_dv, rocket_mass,
                                Some(&state.rocket), Some(&state.design),
                            );
                            clamp_cursor(&mut state.selected, state.destinations.len());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
pub(super) mod help_tests {
    use super::*;
    use ratatui::backend::TestBackend;

    pub(in crate::ui) fn render(app: &App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, app)).unwrap();
        let buf = terminal.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A game with one of everything the panes act on, so that
    /// selection-dependent keys have something to select. Without this
    /// the completeness test below would pass vacuously on an empty
    /// company.
    /// Shared fixture: a company with an engine, a reactor, a rocket
    /// design built from that engine, and one finished rocket in
    /// inventory. Used by the retire tests too.
    pub(in crate::ui) fn app() -> App {
        let mut game = crate::game_state::GameState::new(
            "Help Test".into(), 200_000_000.0, 7,
        );
        let bal = game.balance.clone();
        let company = &mut game.player_company;

        for i in 0..4 {
            company.hire_team(format!("Team {i}"), &bal);
        }
        for i in 0..2 {
            company.hire_manufacturing_team(format!("Mfg {i}"), &bal);
        }
        company.start_engine_project(
            "E1".into(), EngineCycle::GasGenerator,
            PropellantPreset::Kerolox, 1.0, None, &bal,
        );
        company.start_proposed_reactor(
            "R1".into(), 1.0, crate::reactor::EnrichmentLevel::Leu, &bal,
        );
        company.promote_proposed(ProjectRef::Reactor(company.reactor_projects[0].project_id,));

        // A rocket project built from the engine above, plus one
        // finished rocket sitting in inventory for the Launches tab.
        let engine = company.engine_projects[0].design_variant(false);
        let design = crate::rocket::RocketDesign {
            id: crate::rocket::RocketDesignId(1),
            name: "Fixture-1".into(),
            stage_groups: vec![vec![Stage {
                id: StageId(1), name: "S1".into(), engine,
                engine_count: 1,
                propellant_mass_kg: 40_000.0,
                structural_mass_kg: 3_000.0,
                fairing: None,
                power_sources: Vec::new(),
            }]],
        };
        company.start_rocket_project(design.clone(), &bal);
        let rpid = company.rocket_projects[0].project_id;
        company.manufacturing.inventory.rockets.push(
            crate::manufacturing::InventoryRocket {
                item_id: crate::manufacturing::InventoryItemId(1),
                rocket_project_id: rpid,
                design_id: design.id,
                rocket_name: "Fixture-1".into(),
                build_cost: 1_000_000.0,
                revision: 0,
                rocket_flaws: Vec::new(),
            },
        );

        // Staff each project so the "release a team" keys have a team
        // to release.
        company.add_team(ProjectRef::Engine(company.engine_projects[0].project_id));
        company.add_team(ProjectRef::Reactor(company.reactor_projects[0].project_id));
        company.add_team(ProjectRef::Rocket(company.rocket_projects[0].project_id));

        // One contract to bid on.
        game.available_contracts.push(crate::contract::Contract {
            id: crate::contract::ContractId(9001),
            name: "Fixture Sat".into(),
            destination: "leo".into(),
            payload_kg: 400.0,
            payment: 12_000_000.0,
            deadline: crate::calendar::GameDate::new(2001, 12, 1),
            status: crate::contract::ContractStatus::Available,
            market_id: crate::contract::MARKET_RIDESHARE,
            campaign_id: None,
            bid_deadline: Some(crate::calendar::GameDate::new(2001, 6, 1)),
            budget_ceiling: 20_000_000.0,
            player_bid: None,
        });

        // A spacecraft in orbit for the fly/dock/undock keys.
        let rocket = design.instantiate(crate::rocket::RocketId(1), 0.0);
        game.spacecraft.push(crate::game_state::Spacecraft {
            id: crate::game_state::SpacecraftId(1),
            name: "Fixture Craft".into(),
            rocket,
            design,
            location: crate::location::LocationId::of("leo"),
            rocket_project_id: rpid,
            payloads: Vec::new(),
        });

        App::new(game)
    }

    /// The bug a Windows screenshot caught in M5 Task 7: hint lines
    /// were assembled from the help modal's long descriptions, came
    /// out at 302 characters for Engines, and the pane clipped them
    /// mid-word — `[R] Revise discovered flaws and pending impro`.
    /// Four of that tab's eight keys were off-screen at any normal
    /// width.
    ///
    /// Renders for real rather than measuring the table, because the
    /// table being short enough is only half of it: the pane has to
    /// actually contain the result.
    #[test]
    fn pane_hints_fit_and_always_offer_the_help_modal() {
        let engines = Tab::ALL.iter()
            .position(|t| matches!(t, Tab::Engines)).unwrap();
        let mut a = app();
        a.active_tab = engines;

        for width in [160u16, 120, 100, 80, 60] {
            let screen = render(&a, width, 40);
            let hint = screen.lines()
                .find(|l| l.contains(keys::HELP_HINT))
                .unwrap_or_else(|| panic!(
                    "at {width} cols nothing offers {:?} — the pointer to \
                     the full key list must survive any width:\n{screen}",
                    keys::HELP_HINT,
                ))
                .trim_end();

            // Nothing may spill past the pane's right border. A hint
            // that overruns eats the border character, so the border
            // still being there is the check.
            assert!(
                hint.ends_with('│'),
                "at {width} cols the hint overran the pane's right \
                 border: {hint:?}",
            );

            // Clipping is allowed; losing the help pointer is not.
            // `…` marks where the keys were cut, so everything after
            // it must be the pointer that was appended afterwards.
            if let Some((_, tail)) = hint.split_once('…') {
                assert!(
                    tail.contains(keys::HELP_HINT),
                    "at {width} cols the line was clipped and the help \
                     pointer went with it: {hint:?}",
                );
            }
        }

        // At a width that fits everything, the keys really are all
        // there — otherwise the assertions above pass on a line that
        // silently dropped half the table.
        let wide = render(&a, 160, 40);
        for fragment in ["[N] New", "[R] Revise", "[O] Order", "[E] Hire"] {
            assert!(
                wide.contains(fragment),
                "{fragment} missing from a 160-column render:\n{wide}",
            );
        }
    }

    /// Everything a keypress could plausibly change, flattened to a
    /// string so it can grow past Rust's 12-element tuple limit.
    fn observable(a: &App) -> String {
        let c = &a.game.player_company;
        format!(
            "{:?}|{:?}|eng{} rkt{} rct{} team{} mfg{} ord{} inv{} \
             asg{}/{}/{} auto{} money{:.0} contracts{}/{}",
            a.input_mode, a.status_message,
            c.engine_projects.len(), c.rocket_projects.len(),
            c.reactor_projects.len(), c.teams.len(),
            c.manufacturing_teams.len(), c.manufacturing.orders.len(),
            c.manufacturing.inventory.rockets.len(),
            c.engine_projects.iter().map(|p| p.teams_assigned).sum::<u32>(),
            c.rocket_projects.iter().map(|p| p.teams_assigned).sum::<u32>(),
            c.reactor_projects.iter().map(|p| p.teams_assigned).sum::<u32>(),
            c.auto_build_targets.len(), c.money,
            a.game.available_contracts.len(), c.active_contracts.len(),
        )
    }

    /// The documented keys must actually do something. This is the
    /// direction that rots in practice: a key gets renamed or dropped
    /// in `handle_*_key` and the help text quietly lies. Feeding each
    /// documented key to its tab's handler and requiring *some*
    /// observable change catches that.
    ///
    /// It cannot catch the opposite (an undocumented key), which is
    /// why `keys.rs` asks you to add both in one edit.
    #[test]
    fn documented_keys_still_do_something() {
        for (ti, tab) in Tab::ALL.iter().enumerate() {
            for binding in keys::for_tab(*tab) {
                // Take the first key of a "B / A / Enter" style label.
                let label = binding.keys.split(" / ").next().unwrap();
                let key = match label {
                    "Shift+M" => KeyCode::Char('M'),
                    "+" => KeyCode::Char('+'),
                    "-" => KeyCode::Char('-'),
                    l if l.chars().count() == 1 => {
                        KeyCode::Char(l.chars().next().unwrap())
                    }
                    "Enter" => KeyCode::Enter,
                    other => panic!("unhandled key label {other:?} on {}", tab.name()),
                };

                let mut a = app();
                a.active_tab = ti;
                a.focused_pane = FocusedPane::Content;
                let before = observable(&a);
                a.handle_key(key);
                let after = observable(&a);
                assert_ne!(
                    before, after,
                    "{} tab documents [{}] {} but pressing it changed nothing — \
                     either the handler dropped the key or keys.rs is stale",
                    tab.name(), binding.keys, binding.action,
                );
            }
        }
    }

    /// `?` opens the reference, and it describes the tab you were on.
    #[test]
    fn question_mark_opens_help_for_the_current_tab() {
        let mut a = app();
        a.active_tab = Tab::ALL.iter().position(|t| *t == Tab::Launches).unwrap();
        a.handle_key(KeyCode::Char('?'));

        assert!(matches!(a.input_mode, InputMode::Help { .. }), "? should open help");
        let text = render(&a, 120, 44);
        assert!(text.contains("Launches tab"), "help should name the current tab");
        assert!(text.contains("Delta-v planner"), "help should list that tab's keys");
        assert!(text.contains("Everywhere"), "help should list the global keys");
        assert!(text.contains("Quit"), "global keys should include Quit");

        // Any key dismisses.
        a.handle_key(KeyCode::Char('z'));
        assert!(matches!(a.input_mode, InputMode::Normal), "any key should close help");
    }

    /// Help inside the designer describes the designer — and closing it
    /// puts the player back in their half-finished design, not on a tab.
    #[test]
    fn help_in_the_designer_returns_to_the_designer() {
        let mut a = app();
        let state = Box::new(RocketDesignerState::new("Half Built".into()));
        a.input_mode = InputMode::designer(state);

        a.handle_key(KeyCode::Char('?'));
        let text = render(&a, 120, 44);
        assert!(text.contains("Rocket designer"), "should describe the designer");
        assert!(text.contains("Swap the sea-level and vacuum nozzle"),
            "designer help should cover [V]");

        a.handle_key(KeyCode::Esc);
        match &a.input_mode {
            InputMode::RocketDesigner { state, sub: DesignerSubMode::Main } => {
                assert_eq!(state.rocket_name, "Half Built",
                    "the in-progress design must survive a help detour");
            }
            other => panic!("should be back in the designer, got {other:?}"),
        }
    }

    /// Every tab's help renders without overflowing an 80-column
    /// terminal — the reference is useless if it's cut off.
    #[test]
    fn help_fits_an_80_column_terminal() {
        for (ti, tab) in Tab::ALL.iter().enumerate() {
            let mut a = app();
            a.active_tab = ti;
            a.handle_key(KeyCode::Char('?'));
            let text = render(&a, 80, 40);
            for line in text.lines() {
                assert!(line.chars().count() <= 80,
                    "{} help overflows 80 columns", tab.name());
            }
            // The modal actually drew something.
            assert!(text.contains("Keys"), "{} help should render", tab.name());
        }
    }
}

#[cfg(test)]
mod onboarding_tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn render(app: &App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw::draw(frame, app)).unwrap();
        let buf = terminal.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn fresh() -> crate::game_state::GameState {
        crate::game_state::GameState::new("Onboard Co".into(), 200_000_000.0, 7)
    }

    /// A new company opens on the orientation screen; any key dismisses
    /// it and it does not come back.
    #[test]
    fn a_new_game_opens_on_the_intro_and_dismisses_for_good() {
        let mut app = App::new_game(fresh());
        assert!(matches!(app.input_mode, InputMode::Intro));

        let text = render(&app, 100, 40);
        assert!(text.contains("Welcome"), "the intro should render");
        assert!(text.contains("Onboard Co"), "it should name the company");
        assert!(text.contains("fireball"),
            "it should warn that early failures are normal");

        app.handle_key(KeyCode::Char('x'));
        assert!(matches!(app.input_mode, InputMode::Normal));

        // Ticking the game must not bring it back.
        for _ in 0..40 {
            app.game.advance_day();
        }
        assert!(matches!(app.input_mode, InputMode::Normal),
            "the intro is a one-time screen");
    }

    /// Loading a save skips the intro — it's for first-time players,
    /// not every session.
    #[test]
    fn loading_a_game_skips_the_intro() {
        let app = App::new(fresh());
        assert!(matches!(app.input_mode, InputMode::Normal));
        let text = render(&app, 100, 40);
        assert!(!text.contains("Welcome"), "a loaded game shouldn't be lectured");
    }

    /// The Next steps panel appears on Overview for a new company and
    /// names both the tab and the key to press.
    #[test]
    fn overview_shows_next_steps_with_a_tab_and_key() {
        // Deliberately a short terminal: the panel must survive above
        // the fold, which is why it renders before the stats block.
        let app = App::new(fresh());
        let text = render(&app, 100, 30);
        assert!(text.contains("Next steps"), "the panel should render:\n{text}");
        assert!(text.contains("first engine"), "it should suggest the first action");
        assert!(text.contains("Engines tab"), "it should name the tab");
        assert!(text.contains("[N]"), "it should name the key");
    }

    /// Idle engineering teams get put to work by themselves. Before M5
    /// only manufacturing teams did, so the bot had an advantage the
    /// player didn't.
    #[test]
    fn idle_engineering_teams_are_auto_assigned() {
        let mut game = fresh();
        game.player_company.start_engine_project(
            "Booster".into(),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0, None, &game.balance,
        );
        assert_eq!(game.player_company.engine_projects[0].teams_assigned, 0);
        assert!(game.player_company.unassigned_team_count() > 0);

        game.advance_day();

        assert_eq!(game.player_company.unassigned_team_count(), 0,
            "an idle engineering team is pure salary burn");
        assert!(game.player_company.engine_projects[0].teams_assigned > 0);
    }

    /// Auto-revise starts a revision on its own, logs it, and can be
    /// turned off per project.
    #[test]
    fn auto_revise_fires_and_can_be_switched_off() {
        use crate::engine_project::EngineDesignStatus;

        // Run until an engine reaches Testing with a discovered flaw.
        let mut game = fresh();
        game.player_company.start_engine_project(
            "Booster".into(),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0, None, &game.balance,
        );
        let mut saw_auto_revision = false;
        for _ in 0..900 {
            let events = game.advance_day();
            if events.iter().any(|e| matches!(
                e, crate::event::GameEvent::AutoRevisionStarted { .. },
            )) {
                saw_auto_revision = true;
                break;
            }
        }
        assert!(saw_auto_revision,
            "auto-revise should kick in once testing finds a flaw");
        assert!(matches!(
            game.player_company.engine_projects[0].status,
            EngineDesignStatus::Revising { .. },
        ));

        // With the flag off, a discovered flaw is left alone.
        let mut game = fresh();
        game.player_company.start_engine_project(
            "Booster".into(),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0, None, &game.balance,
        );
        game.player_company.engine_projects[0].auto_revise = false;
        let mut ever_revised = false;
        for _ in 0..900 {
            game.advance_day();
            if matches!(
                game.player_company.engine_projects[0].status,
                EngineDesignStatus::Revising { .. },
            ) {
                ever_revised = true;
                break;
            }
        }
        assert!(!ever_revised,
            "auto-revise off means the player decides when to revise");
        assert!(game.player_company.engine_projects[0].discovered_flaw_count() > 0,
            "precondition: a flaw was found and left unrevised");
    }

    /// Pre-M5 saves have no `auto_revise` field; they should load with
    /// it on, matching the default for new projects.
    #[test]
    fn old_saves_default_auto_revise_on() {
        let mut game = fresh();
        game.player_company.start_engine_project(
            "Booster".into(),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0, None, &game.balance,
        );
        let mut json = serde_json::to_value(&game).unwrap();
        // Strip the field the way a pre-M5 save would lack it.
        json["player_company"]["engine_projects"][0]
            .as_object_mut().unwrap().remove("auto_revise");
        let reloaded: crate::game_state::GameState =
            serde_json::from_value(json).expect("pre-M5 save should load");
        assert!(reloaded.player_company.engine_projects[0].auto_revise);
    }
}
