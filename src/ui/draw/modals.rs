//! Modal overlays: help, the intro, the retire confirmation, and
//! `draw_modal`, one arm per `InputMode`.

use super::*;
use crate::ui::DesignerSubMode;

/// The `?` reference. Shows the keys for wherever the player pressed
/// it — the current tab, or the rocket designer — followed by the
/// global keys that work everywhere.
pub(super) fn draw_help_modal(
    frame: &mut Frame,
    scope: &crate::ui::HelpScope,
    screen: Rect,
) {
    use crate::ui::keys;

    let (context_name, bindings): (String, &[keys::KeyBinding]) = match scope {
        crate::ui::HelpScope::Tab(tab) => {
            (format!("{} tab", tab.name()), keys::for_tab(*tab))
        }
        crate::ui::HelpScope::RocketDesigner => (
            "Rocket designer".to_string(), keys::ROCKET_DESIGNER,
        ),
    };

    // Widest key label across both sections, so the two columns line
    // up with each other rather than only within a section.
    let key_width = bindings.iter().chain(keys::GLOBAL)
        .map(|b| b.keys.chars().count())
        .max()
        .unwrap_or(6)
        .max(6);

    let mut lines: Vec<Line> = Vec::new();
    let section = |lines: &mut Vec<Line>, title: &str, bs: &[keys::KeyBinding]| {
        lines.push(Line::from(Span::styled(
            format!("  {}", title),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        if bs.is_empty() {
            lines.push(hint_line("    (nothing beyond the keys below)"));
        }
        for b in bs {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("{:<w$}", b.keys, w = key_width),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw("  "),
                Span::raw(b.action),
            ]));
        }
        lines.push(Line::from(""));
    };

    lines.push(Line::from(""));
    section(&mut lines, &context_name, bindings);
    section(&mut lines, "Everywhere", keys::GLOBAL);
    // No "press any key" line here — the status bar already says it.

    // Size to the content rather than a fixed percentage — a short
    // tab's reference shouldn't render as a mostly-empty box.
    let wanted_h = (lines.len() as u16).saturating_add(2);
    let wanted_w = lines.iter()
        .map(|l| l.spans.iter().map(|sp| sp.content.chars().count()).sum::<usize>())
        .max()
        .unwrap_or(40) as u16
        + 4;
    let h = wanted_h.min(screen.height.saturating_sub(2)).max(3);
    let w = wanted_w.min(screen.width.saturating_sub(2)).max(20);
    let area = Rect {
        x: screen.x + (screen.width.saturating_sub(w)) / 2,
        y: screen.y + (screen.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, area);
    render_modal(frame, area, " Keys ", lines);
}

/// One-screen orientation on a new game. Deliberately short: it says
/// what the company is, what the first move is, and where the keys
/// live. Everything else the player discovers, which is the point of
/// the game.
pub(super) fn draw_intro_modal(frame: &mut Frame, app: &App, screen: Rect) {
    let name = &app.game.player_company.name;
    let money = format_money(app.game.player_company.money);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  You are running "),
            Span::styled(name.clone(), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(", a new rocket company with {money}.")),
        ]),
        Line::from(""),
        Line::from("  Rockets are built from engines you design yourself. Engines"),
        Line::from("  take months to develop and arrive with hidden flaws, which"),
        Line::from("  testing reveals and revisions fix. Flying an unrevised design"),
        Line::from("  usually ends in a fireball — that is normal, and survivable."),
        Line::from(""),
        Line::from("  Income comes from launch contracts you bid for against a"),
        Line::from("  competitor. Bid too high and you lose the work; too low and"),
        Line::from("  you fly at a loss. Nobody tells you the customer's budget."),
        Line::from(""),
        Line::from(vec![
            Span::raw("  The "),
            Span::styled("Next steps", Style::default().fg(Color::Cyan)),
            Span::raw(" box on the Overview tab suggests what to do"),
        ]),
        Line::from("  first. It is only a suggestion."),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ? at any time for the keys. ",
                Style::default().fg(Color::Cyan)),
            Span::styled("Any key to begin.",
                Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
    ];

    let h = (lines.len() as u16 + 2).min(screen.height);
    let w = 68u16.min(screen.width);
    let area = Rect {
        x: screen.x + (screen.width.saturating_sub(w)) / 2,
        y: screen.y + (screen.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, area);
    render_modal(frame, area, " Welcome ", lines);
}

pub(super) fn draw_modal(frame: &mut Frame, app: &App, area: Rect) {
    let modal_area = centered_rect(60, 50, area);
    frame.render_widget(Clear, modal_area);

    match &app.input_mode {
        InputMode::Normal => {}
        InputMode::Help { tab } => {
            draw_help_modal(frame, &crate::ui::HelpScope::Tab(*tab), area);
        }
        InputMode::Intro => draw_intro_modal(frame, app, area),
        InputMode::ConfirmRetire { effects, .. } => {
            draw_confirm_retire_modal(frame, effects, area);
        }
        InputMode::EngineEditor { project_id, cursor, state } => {
            draw_engine_editor_modal(frame, app, *project_id, *cursor, None, state.is_none(), modal_area);
        }
        InputMode::EngineEditorField { project_id, cursor, field, buffer, state } => {
            draw_engine_editor_modal(frame, app, *project_id, *cursor, Some((field.label(), buffer.clone())), state.is_none(), modal_area);
        }
        InputMode::ReactorEditor { project_id, cursor } => {
            draw_reactor_editor_modal(frame, app, *project_id, *cursor, None, modal_area);
        }
        InputMode::ReactorEditorField { project_id, cursor, field, buffer } => {
            draw_reactor_editor_modal(frame, app, *project_id, *cursor, Some((field.label(), buffer.clone())), modal_area);
        }
        InputMode::SelectThirdParty { selected } => {
            let catalog = &app.game.player_company.third_party_catalog;
            let mut lines = vec![
                Line::from("  Available third-party engines:"),
                Line::from(""),
            ];
            for (i, entry) in catalog.iter().enumerate() {
                let marker = if i == *selected { "▶" } else { " " };
                let style = selected_style(i == *selected);
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {} {}  {:.0}kN  {:.0}s  {}/unit",
                        marker,
                        entry.design.name,
                        entry.design.thrust_n / 1000.0,
                        entry.design.isp_s,
                        format_money(entry.purchase_cost_per_unit),
                    ),
                    style,
                )));
            }
            render_modal(frame, modal_area, " Contract Third-Party Engine ", lines);
        }
        InputMode::RocketName { buffer } => {
            let lines = vec![
                Line::from(""),
                Line::from("  Enter rocket name:"),
                Line::from(""),
                Line::from(format!("  > {}█", buffer)),
            ];
            render_modal(frame, modal_area, " New Rocket Design ", lines);
        }
        InputMode::BidEntry { contract_index, buffer, basis } => {
            draw_bid_entry_modal(frame, app, *contract_index, buffer, basis, area);
        }
        InputMode::BidRules { selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Standing bid rules: auto-bid build cost × (1 + margin)"),
                Line::from("  on new solicitations while a rocket is free to fly them."),
                Line::from("  Space toggles, ←/→ adjust margin, Esc closes."),
                Line::from(""),
            ];
            let markets: Vec<&crate::contract::Market> = app.game.markets.iter()
                .filter(|m| m.active)
                .collect();
            for (i, market) in markets.iter().enumerate() {
                let rule = app.game.player_company.bid_rules.get(&market.id);
                let (state, margin) = match rule {
                    Some(r) if r.enabled => ("auto", r.margin),
                    Some(r) => (" off", r.margin),
                    None => (" off", crate::game_state::BidRule::default().margin),
                };
                let marker = if i == *selected { "▶ " } else { "  " };
                let mut line = Line::from(format!(
                    "  {marker}{:<28} [{state}]  margin +{:.0}%",
                    market.name, margin * 100.0,
                ));
                if state == "auto" {
                    line = line.style(Style::default().fg(Color::Green));
                }
                lines.push(line);
            }
            if markets.is_empty() {
                lines.push(Line::from("  (no active markets)"));
            }
            render_modal(frame, modal_area, " Bid Rules ", lines);
        }
        InputMode::AwardHistory { scroll } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Observed awards, newest first (↑/↓ scroll, Esc closes):"),
                Line::from(""),
            ];
            let visible = (modal_area.height as usize).saturating_sub(6);
            let records: Vec<&crate::contract::AwardRecord> =
                app.game.award_history.iter().rev().skip(*scroll).take(visible.max(1)).collect();
            for r in &records {
                let market_entry = app.game.markets.iter().find(|m| m.id == r.market_id);
                let market: String = market_entry
                    .map(|m| fit(&m.name, 18))
                    .unwrap_or_else(|| "?".into());
                // Short destination tag ("GTO"), not the long display
                // name — these rows are tight.
                let dest: String = market_entry
                    .and_then(|m| m.destinations.iter()
                        .find(|d| d.location_id == r.destination)
                        .map(|d| d.display_name.clone()))
                    .unwrap_or_else(|| r.destination.to_uppercase());
                let (outcome, color) = match &r.outcome {
                    crate::contract::AwardOutcome::PlayerWon { amount } => (
                        format!("won at {}", format_money(*amount)),
                        Color::Green,
                    ),
                    crate::contract::AwardOutcome::CompetitorWon { company, amount, player_bid } => {
                        match player_bid {
                            Some(b) => (
                                format!("{} {} (you {})",
                                    company, format_money(*amount), format_money(*b)),
                                Color::Red,
                            ),
                            None => (
                                format!("{} {}", company, format_money(*amount)),
                                Color::DarkGray,
                            ),
                        }
                    }
                    crate::contract::AwardOutcome::PlayerRejected { bid } => (
                        format!("over budget (bid {})", format_money(*bid)),
                        Color::Yellow,
                    ),
                };
                // Campaign block awards: amounts are per mission, so
                // tag the row with the block size.
                let block_tag = r.missions
                    .map(|n| format!(" x{n}"))
                    .unwrap_or_default();
                lines.push(Line::from(format!(
                    "  {}  {:<18} {:>6.0} kg →{:<4} {}{}",
                    r.date.iso(),
                    market, r.payload_kg, dest, outcome, block_tag,
                )).style(Style::default().fg(color)));
            }
            if app.game.award_history.is_empty() {
                lines.push(Line::from("  (no awards observed yet — bid on a solicitation)"));
            }
            render_modal(frame, modal_area, " Award History ", lines);
        }
        InputMode::Campaigns { selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Anchor-customer programs: one sealed price per mission"),
                Line::from("  wins the whole block. Enter/B bids on a soliciting"),
                Line::from("  program, ↑/↓ select, Esc closes."),
                Line::from(""),
            ];
            for (i, c) in app.game.active_campaigns.iter().enumerate() {
                let market: String = app.game.markets.iter()
                    .find(|m| m.id == c.market_id)
                    .map(|m| fit(&m.name, 18))
                    .unwrap_or_else(|| "?".into());
                let marker = if i == *selected { "▶ " } else { "  " };
                // Status is strictly public knowledge: bids close /
                // your own sealed bid / the announced winning price.
                // The hidden reference and ceiling never render.
                let (status, color) = match &c.status {
                    crate::contract::CampaignStatus::Soliciting { bid_deadline, player_bid, .. } => {
                        let bid = player_bid
                            .map(|b| format!("  your bid {}/ea", format_money(b)))
                            .unwrap_or_default();
                        (format!(
                            "bids close {}{}",
                            bid_deadline.iso(), bid,
                        ), Color::Yellow)
                    }
                    crate::contract::CampaignStatus::Won { by_player, company } => {
                        let holder = if *by_player { "yours" } else { company.as_str() };
                        let strikes = if c.missions_missed > 0 {
                            format!("  ▲{} missed", c.missions_missed)
                        } else {
                            String::new()
                        };
                        (format!(
                            "{holder} at {}/ea — flight {}/{}{}",
                            format_money(c.payment_per_mission),
                            c.missions_issued, c.missions_total, strikes,
                        ), if *by_player { Color::Green } else { Color::Red })
                    }
                };
                lines.push(Line::from(format!(
                    "  {marker}{:<24} {:<18} {:>6.0} kg →{:<4} x{}",
                    fit(&c.name, 24),
                    market, c.payload_kg, c.destination_display, c.missions_total,
                )));
                lines.push(Line::from(format!("        {status}"))
                    .style(Style::default().fg(color)));
            }
            if app.game.active_campaigns.is_empty() {
                lines.push(Line::from("  (no active programs — announcements appear in Events)"));
            }
            render_modal(frame, modal_area, " Programs ", lines);
        }
        InputMode::CampaignBidEntry { campaign_id, buffer, .. } => {
            let (name, missions) = app.game.active_campaigns.iter()
                .find(|c| c.id == *campaign_id)
                .map(|c| (c.name.clone(), c.missions_total))
                .unwrap_or_default();
            let total = buffer.trim().parse::<f64>().ok()
                .filter(|m| *m > 0.0)
                .map(|m| format!(
                    "  Block total: {} over {} missions",
                    format_money(m * 1_000_000.0 * missions as f64), missions,
                ))
                .unwrap_or_default();
            let lines = vec![
                Line::from(""),
                Line::from(format!("  {} — {} missions", name, missions)),
                Line::from(""),
                Line::from("  Enter sealed price per mission in $M"),
                Line::from("  (Enter to submit, Esc to cancel):"),
                Line::from(""),
                Line::from(format!("  > {}█  ($M per mission)", buffer)),
                Line::from(total),
            ];
            render_modal(frame, modal_area, " Block Bid ", lines);
        }
        InputMode::LaunchManifest {
            rocket_item_id, contract_picks, spacecraft_picks,
            spacecraft_item_ids, cursor, ..
        } => {
            let contracts = &app.game.player_company.active_contracts;
            let inventory = &app.game.player_company.manufacturing.inventory;

            let carrier_name = inventory.rockets.iter()
                .find(|r| r.item_id == *rocket_item_id)
                .map(|r| r.rocket_name.clone())
                .unwrap_or_else(|| "(unknown)".into());

            // Compute manifest summary: destination + total payload mass.
            let mut destination: Option<String> = None;
            let mut destination_conflict = false;
            for (i, p) in contract_picks.iter().enumerate() {
                if !p { continue; }
                let dest = &contracts[i].destination;
                match &destination {
                    None => destination = Some(dest.clone()),
                    Some(d) if d == dest => {}
                    Some(_) => destination_conflict = true,
                }
            }
            let destination_for_summary = destination.clone()
                .unwrap_or_else(|| "leo".to_string());

            let mut payload_mass = 0.0;
            for (i, p) in contract_picks.iter().enumerate() {
                if *p { payload_mass += contracts[i].payload_kg; }
            }
            for (i, p) in spacecraft_picks.iter().enumerate() {
                if !p { continue; }
                let item_id = spacecraft_item_ids[i];
                if let Some(r) = inventory.rockets.iter().find(|r| r.item_id == item_id) {
                    if let Some(rp) = app.game.player_company.rocket_projects.iter()
                        .find(|rp| rp.project_id == r.rocket_project_id)
                    {
                        payload_mass += rp.design.total_mass_kg();
                    }
                }
            }

            let mut lines = vec![
                Line::from(""),
                Line::from(format!("  Carrier: {}", carrier_name)),
                Line::from(format!(
                    "  Destination: {}{}",
                    contract::destination_display_name(&destination_for_summary),
                    if destination_conflict { "  ▲ contracts disagree" } else { "" },
                )),
                Line::from(format!("  Payload mass: {}", format_mass(payload_mass))),
                Line::from(""),
            ];

            let mut row = 0usize;

            if !contracts.is_empty() {
                lines.push(hint_line("  ── Contracts ──"));
                for (i, c) in contracts.iter().enumerate() {
                    let mark = if *cursor == row { " ▶ " } else { "   " };
                    let check = if contract_picks[i] { "[✓]" } else { "[ ]" };
                    let style = selected_style(*cursor == row);
                    // No destination here: every contract name is built as
                    // "<something> to <destination>", so spelling it out
                    // again just reads as "… to LEO → Low Earth Orbit". The
                    // summary above still names the manifest's destination
                    // and flags contracts that disagree.
                    lines.push(Line::from(Span::styled(
                        format!("{}{} {} ({:.0} kg, {})",
                            mark, check, c.name, c.payload_kg, format_money(c.payment)),
                        style,
                    )));
                    row += 1;
                }
                lines.push(Line::from(""));
            }

            if !spacecraft_item_ids.is_empty() {
                lines.push(hint_line("  ── Spacecraft Payloads ──"));
                for (i, item_id) in spacecraft_item_ids.iter().enumerate() {
                    let mark = if *cursor == row { " ▶ " } else { "   " };
                    let check = if spacecraft_picks[i] { "[✓]" } else { "[ ]" };
                    let style = selected_style(*cursor == row);
                    let (name, mass) = inventory.rockets.iter()
                        .find(|r| r.item_id == *item_id)
                        .and_then(|r| {
                            app.game.player_company.rocket_projects.iter()
                                .find(|rp| rp.project_id == r.rocket_project_id)
                                .map(|rp| (r.rocket_name.clone(), rp.design.total_mass_kg()))
                        })
                        .unwrap_or_else(|| ("(unknown)".into(), 0.0));
                    lines.push(Line::from(Span::styled(
                        format!("{}{} {} ({})", mark, check, name, format_mass(mass)),
                        style,
                    )));
                    row += 1;
                }
                lines.push(Line::from(""));
            }

            if contracts.is_empty() && spacecraft_item_ids.is_empty() {
                lines.push(Line::from("  (no contracts or spacecraft available — Enter for test launch)"));
            }
            lines.push(Line::from(""));
            lines.push(hint_line("  [Space] toggle  [Enter] launch  [Esc] cancel"));

            render_modal(frame, modal_area, " Launch Manifest ", lines);
        }
        InputMode::LaunchResult { record } => {
            let mut lines = vec![
                Line::from(""),
            ];
            let outcome_line = match &record.outcome {
                LaunchOutcome::Success => Line::from(Span::styled(
                    "  LAUNCH SUCCESS",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                )),
                LaunchOutcome::PartialFailure { reason } => Line::from(Span::styled(
                    format!("  PARTIAL FAILURE: {}", reason),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )),
                LaunchOutcome::Failure { reason } => Line::from(Span::styled(
                    format!("  LAUNCH FAILURE: {}", reason),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
            };
            lines.push(outcome_line);
            lines.push(Line::from(""));
            let dest_name = contract::destination_display_name(&record.destination);
            lines.push(Line::from(format!("  {} → {}", record.rocket_name, dest_name)));
            if record.payload_kg > 0.0 {
                lines.push(Line::from(format!("  Payload: {}", format_mass(record.payload_kg))));
            }
            lines.push(Line::from(""));

            if !record.flaws_activated.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  Flaws activated:",
                    Style::default().fg(Color::Red),
                )));
                for flaw in &record.flaws_activated {
                    lines.push(Line::from(format!("    {} ({}): {}",
                        flaw.engine_name, flaw.consequence, flaw.flaw_description)));
                }
                lines.push(Line::from(""));
            }

            lines.push(hint_line("  Press any key to continue"));

            render_modal(frame, modal_area, " Launch Result ", lines);
        }
        InputMode::PlannerSetup { state } => {
            use crate::ui::{PlannerSetupField};
            let mut lines = vec![
                Line::from(""),
                Line::from("  Δv Planner Setup"),
                Line::from(""),
            ];

            // Design field
            let design_label = if state.active_field == PlannerSetupField::Design {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(Span::styled("  Rocket Design:", design_label)));
            for (i, &pi) in state.eligible_projects.iter().enumerate() {
                let rp = &app.game.player_company.rocket_projects[pi];
                let marker = if i == state.selected_project { " ▶ " } else { "   " };
                let style = selected_style(i == state.selected_project && state.active_field == PlannerSetupField::Design);
                lines.push(Line::from(Span::styled(
                    format!("  {}{}", marker, rp.design.name),
                    style,
                )));
            }
            lines.push(Line::from(""));

            // Payload field
            let payload_label = if state.active_field == PlannerSetupField::Payload {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let cursor = if state.active_field == PlannerSetupField::Payload { "▏" } else { "" };
            lines.push(Line::from(vec![
                Span::styled("  Payload (kg): ", payload_label),
                Span::styled(
                    format!("{}{}", state.payload_buffer, cursor),
                    if state.active_field == PlannerSetupField::Payload {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default()
                    },
                ),
            ]));
            lines.push(Line::from(""));

            // Location field
            let loc_label = if state.active_field == PlannerSetupField::Location {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(Span::styled("  Start Location:", loc_label)));
            for (i, (_, display_name)) in state.locations.iter().enumerate() {
                let marker = if i == state.selected_location { " ▶ " } else { "   " };
                let style = selected_style(i == state.selected_location && state.active_field == PlannerSetupField::Location);
                lines.push(Line::from(Span::styled(
                    format!("  {}{}", marker, display_name),
                    style,
                )));
            }

            lines.push(Line::from(""));
            lines.push(hint_line("  [Tab] Switch field  [Enter] Start  [Esc] Cancel"));

            render_modal(frame, modal_area, " Δv Planner Setup ", lines);
        }
        InputMode::DvPlanner { state } => {
            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
            let loc_name = contract::destination_display_name(state.current_location.name());
            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("  Location: "),
                    Span::styled(loc_name, Style::default().fg(Color::Cyan)),
                    Span::raw(format!("    Δv: {}", format_dv(remaining_dv))),
                    Span::raw(format!("    Payload: {:.0} kg", state.payload_kg)),
                ]),
                Line::from(""),
            ];

            // Show planned route so far
            if !state.actions.is_empty() {
                lines.push(hint_line("  Route:"));
                for action in &state.actions {
                    match action {
                        crate::ui::PlanAction::Leg { to_display, dv_cost, .. } => {
                            lines.push(Line::from(format!(
                                "    → {} (Δv: {:.0})", to_display, dv_cost,
                            )));
                        }
                        crate::ui::PlanAction::DropPayload { mass_dropped } => {
                            lines.push(Line::from(Span::styled(
                                format!("    ✦ Drop payload ({:.0} kg)", mass_dropped),
                                Style::default().fg(Color::Yellow),
                            )));
                        }
                    }
                }
                lines.push(Line::from(""));
            }

            // Destinations
            lines.push(hint_line("  Destinations:"));
            if state.destinations.is_empty() {
                lines.push(Line::from("  (no reachable destinations)"));
            } else {
                for (i, (_, display_name, dv_cost)) in state.destinations.iter().enumerate() {
                    let marker = if i == state.selected { " ▶ " } else { "   " };
                    let style = selected_style(i == state.selected);
                    let dv_after = remaining_dv - dv_cost;
                    lines.push(Line::from(vec![
                        Span::styled(format!("{}{}", marker, display_name), style),
                        Span::styled(
                            format!("  Δv: {:.0}  rem: {:.0}", dv_cost, dv_after),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
            }

            lines.push(Line::from(""));
            lines.push(hint_line("  [Enter] Go  [D] Drop payload  [U] Undo  [Esc] Close"));

            render_modal(frame, modal_area, " Δv Planner ", lines);
        }
        InputMode::FlySelectSpacecraft { selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Select spacecraft:"),
                Line::from(""),
            ];
            for (i, sc) in app.game.spacecraft.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = selected_style(i == *selected);
                let dv = sc.remaining_delta_v();
                lines.push(Line::from(vec![
                    Span::styled(format!("{}{}", marker, sc.name), style),
                    Span::styled(
                        format!("  @ {}  Δv: {}", sc.location, format_dv(dv)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(hint_line("  [Enter] Select  [Esc] Cancel"));
            render_modal(frame, modal_area, " Fly Spacecraft ", lines);
        }
        InputMode::FlySelectDestination { destinations, remaining_dv, selected, .. } => {
            let mut lines = vec![
                Line::from(""),
                Line::from(format!("  Remaining Δv: {}", format_dv(*remaining_dv))),
                Line::from(""),
                Line::from("  Select destination:"),
                Line::from(""),
            ];
            for (i, (_, display_name, dv_cost)) in destinations.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = selected_style(i == *selected);
                let dv_after = remaining_dv - dv_cost;
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}{}", marker, display_name),
                        style,
                    ),
                    Span::styled(
                        format!("  Δv: {:.0}  ", dv_cost),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("remaining: {:.0}", dv_after),
                        if dv_after < 500.0 {
                            Style::default().fg(Color::Red)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                ]));
            }
            render_modal(frame, modal_area, " Fly Spacecraft ", lines);
        }
        InputMode::DockSelectSmall { selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Pick spacecraft to dock onto another:"),
                Line::from(""),
            ];
            for (i, sc) in app.game.spacecraft.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = selected_style(i == *selected);
                let loc = contract::destination_display_name(sc.location.name());
                lines.push(Line::from(Span::styled(
                    format!("{}{}  @ {}", marker, sc.name, loc),
                    style,
                )));
            }
            lines.push(Line::from(""));
            lines.push(hint_line("  [Enter] Select  [Esc] Cancel"));
            render_modal(frame, modal_area, " Dock — Pick Spacecraft ", lines);
        }
        InputMode::DockSelectLarge { small_idx, candidates, selected } => {
            let small_name = &app.game.spacecraft[*small_idx].name;
            let small_loc = contract::destination_display_name(
                app.game.spacecraft[*small_idx].location.name());
            let mut lines = vec![
                Line::from(""),
                Line::from(format!("  Dock {} onto … (at {})", small_name, small_loc)),
                Line::from(""),
            ];
            for (i, &cand) in candidates.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = selected_style(i == *selected);
                let sc = &app.game.spacecraft[cand];
                lines.push(Line::from(Span::styled(
                    format!("{}{}", marker, sc.name),
                    style,
                )));
            }
            lines.push(Line::from(""));
            lines.push(hint_line("  [Enter] Confirm  [Esc] Cancel"));
            render_modal(frame, modal_area, " Dock — Pick Carrier ", lines);
        }
        InputMode::UndockSelectCarrier { candidates, selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Pick carrier to undock from:"),
                Line::from(""),
            ];
            for (i, &cand) in candidates.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = selected_style(i == *selected);
                let sc = &app.game.spacecraft[cand];
                let loc = contract::destination_display_name(sc.location.name());
                lines.push(Line::from(Span::styled(
                    format!("{}{}  @ {}", marker, sc.name, loc),
                    style,
                )));
            }
            lines.push(Line::from(""));
            lines.push(hint_line("  [Enter] Select  [Esc] Cancel"));
            render_modal(frame, modal_area, " Undock — Pick Carrier ", lines);
        }
        InputMode::UndockSelectPayload { carrier_idx, payload_indices, selected } => {
            let carrier = &app.game.spacecraft[*carrier_idx];
            let mut lines = vec![
                Line::from(""),
                Line::from(format!("  Undock from {}:", carrier.name)),
                Line::from(""),
            ];
            for (i, &pi) in payload_indices.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = selected_style(i == *selected);
                if let crate::flight::Payload::Spacecraft { name, .. } = &carrier.payloads[pi] {
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", marker, name),
                        style,
                    )));
                }
            }
            lines.push(Line::from(""));
            lines.push(hint_line("  [Enter] Confirm  [Esc] Cancel"));
            render_modal(frame, modal_area, " Undock — Pick Payload ", lines);
        }
        InputMode::RocketDesigner { state, sub } => match sub {
            DesignerSubMode::Main => {}
            DesignerSubMode::Help => {
                draw_help_modal(frame, &crate::ui::HelpScope::RocketDesigner, area);
            }
            DesignerSubMode::PickEngine(pick) => {
                draw_rocket_pick_engine_modal(frame, app, state, pick.selected, modal_area);
            }
            DesignerSubMode::PowerEditor { group_index, stage_index, cursor } => {
                draw_power_editor_modal(
                    frame, app, state, *group_index, *stage_index, *cursor, modal_area,
                );
            }
            DesignerSubMode::PayloadInput { buffer } => {
                let lines = vec![
                    Line::from(""),
                    Line::from("  Enter payload mass (kg):"),
                    Line::from(""),
                    Line::from(format!("  > {}█", buffer)),
                ];
                render_modal(frame, modal_area, " Set Payload ", lines);
            }
            DesignerSubMode::LocationPicker { target, locations, selected } => {
                let title = match target {
                    crate::ui::LocationPickerTarget::LaunchSite => " Pick Launch Site ",
                    crate::ui::LocationPickerTarget::MissionDestination => " Pick Mission Destination ",
                };
                let mut lines = vec![Line::from("")];
                // Visible window around the selected entry so long lists scroll.
                let modal_inner_h = modal_area.height.saturating_sub(4) as usize;
                let window = modal_inner_h.max(5);
                let start = selected.saturating_sub(window / 2).min(locations.len().saturating_sub(window));
                for (i, (_id, name)) in locations.iter().enumerate().skip(start).take(window) {
                    let marker = if i == *selected { " ▶ " } else { "   " };
                    let style = selected_style(i == *selected);
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", marker, name),
                        style,
                    )));
                }
                lines.push(Line::from(""));
                lines.push(hint_line("  [↑↓] Move  [Enter] Confirm  [Esc] Cancel"));
                render_modal(frame, modal_area, title, lines);
            }
        },
    }
}

/// "Are you sure?" before retiring a design.
///
/// Spells out every consequence the player can't otherwise see — teams
/// coming back, orders being scrapped with no refund — and says plainly
/// what *isn't* affected, because "retire" next to a list of cancelled
/// builds reads more destructive than it is.
pub(super) fn draw_confirm_retire_modal(
    frame: &mut Frame, effects: &crate::company::RetirementEffects, area: Rect,
) {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Retire {}?", effects.design_name),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let mut consequences: Vec<String> = Vec::new();
    if effects.teams_released > 0 {
        consequences.push(format!(
            "{} engineering team(s) will be released.", effects.teams_released));
    }
    if effects.auto_build_cleared {
        consequences.push("Auto-build will be switched off.".to_string());
    }
    if !effects.cancelled.is_empty() {
        consequences.push(format!(
            "{} build order(s) will be cancelled — no refund.",
            effects.cancelled.len(),
        ));
    }
    if !effects.reassigned_engine_orders.is_empty() {
        consequences.push(format!(
            "{} engine build(s) carry on — other designs use them.",
            effects.reassigned_engine_orders.len(),
        ));
    }
    for c in &consequences {
        lines.push(Line::from(Span::styled(
            format!("  {}", c),
            Style::default().fg(Color::Yellow),
        )));
    }
    if !consequences.is_empty() {
        lines.push(Line::from(""));
    }

    lines.push(hint_line("  Rockets already built keep flying, and it stays"));
    lines.push(hint_line("  on past launch records. This can't be undone."));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Y] Retire    [Esc] Cancel",
        Style::default().fg(Color::Cyan),
    )));

    // Sized to the content rather than a fixed percentage: the prompt is
    // between six and twelve lines depending on what it has to warn
    // about, and a half-screen box for two of them looks alarming.
    let height = (lines.len() as u16 + 2).min(area.height);
    let width = 56.min(area.width);
    let modal_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, modal_area);
    render_modal(frame, modal_area, " Retire design ", lines);
}

/// The Place Bid modal: what the mission is, when bids close, which
/// rocket would fly it and what that costs, then the field, prefilled
/// with the rule engine's price. A first-time bidder should be able to
/// press Enter and have bid something sane.
fn draw_bid_entry_modal(
    frame: &mut Frame,
    app: &App,
    contract_index: usize,
    buffer: &str,
    basis: &crate::game_state::BidBasis,
    screen: Rect,
) {
    use crate::game_state::CostBasis;
    // Its own box: the default modal is 60% of the screen, which at 80
    // columns clips the cost line. Every line here is under 50 chars.
    let w = 60u16.min(screen.width.saturating_sub(4));
    let h = 18u16.min(screen.height.saturating_sub(2));
    let area = Rect {
        x: screen.x + (screen.width.saturating_sub(w)) / 2,
        y: screen.y + (screen.height.saturating_sub(h)) / 2,
        width: w, height: h,
    };
    frame.render_widget(Clear, area);
    let Some(c) = app.game.available_contracts.get(contract_index) else {
        render_modal(frame, area, " Place Bid ", vec![Line::from("  (contract gone)")]);
        return;
    };
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {}", c.name), Style::default().add_modifier(Modifier::BOLD))),
        Line::from(format!(
            "  {} to {}",
            format_kg(c.payload_kg),
            crate::contract::destination_display_name(&c.destination),
        )),
    ];
    if let Some(close) = c.bid_deadline {
        lines.push(Line::from(format!("  Bids close {} (today {})", close, app.game.date)));
    }
    lines.push(Line::from(""));
    match (&basis.rocket, basis.cost) {
        (Some(rocket), Some((cost, how))) => {
            lines.push(Line::from(format!("  Your rocket:    {}", rocket)));
            let how = match how {
                CostBasis::BuildHistory => "mean of your last builds",
                CostBasis::MaterialEstimate => "materials for a first build",
            };
            lines.push(Line::from(format!("  Marginal cost:  {}", format_money(cost))));
            lines.push(hint_line(format!("                  ({how})")));
        }
        _ => {
            lines.push(Line::from(Span::styled(
                "  No design of yours can lift this yet.",
                Style::default().fg(Color::Red),
            )));
        }
    }
    if let Some(s) = basis.suggested {
        lines.push(Line::from(format!(
            "  Suggested:      ${}M  (cost × {:.2})", crate::ui::millions_text(s), 1.0 + basis.margin,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(format!("  Bid: > {}█  ($M)", buffer)));
    lines.push(Line::from(""));
    lines.push(hint_line("  The customer's budget is hidden. Higher bids"));
    lines.push(hint_line("  win less often; the award is sealed until"));
    lines.push(hint_line("  the bid date."));
    lines.push(hint_line("  [Enter] Submit  [Esc] Cancel"));
    render_modal(frame, area, " Place Bid ", lines);
}
