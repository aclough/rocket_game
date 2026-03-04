use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::contract;
use crate::engine::EngineCycle;
use crate::engine_project::{self, EngineDesignStatus, EngineSource, PropellantPreset};
use crate::event::EventImportance;
use crate::flaw::FlawConsequence;
use crate::launch::LaunchOutcome;
use crate::location::DELTA_V_MAP;
use crate::rocket;
use crate::rocket_project;
use crate::ui::{App, FocusedPane, InputMode, RocketDesignerState, Tab};

/// Draw the entire application frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Check if we're in the rocket designer — it replaces the full UI
    if let InputMode::RocketDesigner { state } = &app.input_mode {
        draw_rocket_designer_full(frame, app, state, size);
        return;
    }

    // Top-level layout: status bar, main area, event feed, help bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),    // status bar
            Constraint::Min(8),      // main area (sidebar + content)
            Constraint::Length(6),   // event feed
            Constraint::Length(3),    // help bar
        ])
        .split(size);

    draw_status_bar(frame, app, outer[0]);
    draw_main_area(frame, app, outer[1]);
    draw_event_feed(frame, app, outer[2]);
    draw_help_bar(frame, app, outer[3]);

    // Draw modal overlay if in input mode
    if !matches!(app.input_mode, InputMode::Normal) {
        draw_modal(frame, app, size);
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let game = &app.game;
    let speed_str = format!("{} {}", game.speed.display_symbol(), game.speed.display_name());
    let money_str = format_money(game.player_company.money);
    let teams_str = format!("Teams: {}", game.player_company.team_count());
    let text = format!(
        "  {}      {}      {}      {}      {}",
        game.player_company.name,
        game.date,
        money_str,
        teams_str,
        speed_str,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Rocket Tycoon ");
    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_main_area(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(16),  // sidebar
            Constraint::Min(30),     // content
        ])
        .split(area);

    draw_sidebar(frame, app, chunks[0]);
    draw_content(frame, app, chunks[1]);
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let highlight_style = if app.focused_pane == FocusedPane::Sidebar {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    let items: Vec<ListItem> = Tab::ALL.iter().enumerate().map(|(i, tab)| {
        let style = if i == app.active_tab {
            highlight_style
        } else {
            Style::default().fg(Color::DarkGray)
        };
        ListItem::new(format!(" {} ", tab.name())).style(style)
    }).collect();

    let border_style = if app.focused_pane == FocusedPane::Sidebar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_style(border_style));
    frame.render_widget(list, area);
}

fn draw_content(frame: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.focused_pane == FocusedPane::Content {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    match app.current_tab() {
        Tab::Overview => draw_overview(frame, app, area, border_style),
        Tab::Teams => draw_teams_tab(frame, app, area, border_style),
        Tab::Engines => draw_engines_tab(frame, app, area, border_style),
        Tab::Rockets => draw_rockets_tab(frame, app, area, border_style),
        Tab::Manufacturing => draw_manufacturing_tab(frame, app, area, border_style),
        Tab::Contracts => draw_contracts_tab(frame, app, area, border_style),
        Tab::Launches => draw_launches_tab(frame, app, area, border_style),
        Tab::Finance => draw_finance_tab(frame, app, area, border_style),
        Tab::Events => draw_events_tab(frame, app, area, border_style),
    }
}

fn draw_overview(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let game = &app.game;
    let lines = vec![
        Line::from(format!("  Company:  {}", game.player_company.name)),
        Line::from(format!("  Founded:  {}", game.start_date)),
        Line::from(format!("  Today:    {}", game.date)),
        Line::from(format!("  Elapsed:  {} days", game.elapsed_days())),
        Line::from(""),
        Line::from(format!("  Money:    {}", format_money(game.player_company.money))),
        Line::from(""),
        Line::from(format!("  Eng. teams:      {}", game.player_company.team_count())),
        Line::from(format!("  Mfg. teams:      {}", game.player_company.manufacturing_teams.len())),
        Line::from(format!("  Engine projects: {}", game.player_company.engine_projects.len())),
        Line::from(format!("  Rocket projects: {}", game.player_company.rocket_projects.len())),
        Line::from(format!("  Mfg. orders:     {}", game.player_company.manufacturing.orders.len())),
        Line::from(format!("  Rockets built:   {}", game.player_company.manufacturing.inventory.rockets.len())),
        Line::from(format!("  Contracts:       {} available, {} accepted",
            game.available_contracts.len(),
            game.player_company.active_contracts.len())),
        Line::from(format!("  Launches:        {}", game.player_company.launch_history.len())),
        Line::from(format!("  Reputation:      {:.0}", game.player_company.reputation.total())),
        Line::from(""),
        Line::from(format!("  Seed:  {}", game.seed.seed())),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Overview ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_teams_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let mut lines = vec![
        Line::from(format!(
            "  All Teams          Monthly cost: {}",
            format_money(company.monthly_salary_cost()),
        )),
        Line::from("  ─────────────────────────────────────────────"),
        Line::from(format!("  Engineering:    {} ({} unassigned)",
            company.team_count(), company.unassigned_team_count())),
        Line::from(format!("  Manufacturing:  {} ({} unassigned)",
            company.manufacturing_teams.len(),
            company.unassigned_manufacturing_team_count())),
        Line::from(""),
    ];

    // Show engineering assignment breakdown
    for project in &company.engine_projects {
        if project.teams_assigned > 0 {
            lines.push(Line::from(format!(
                "    {} eng. team(s) on \"{}\"  (rate: {:.2}/day)",
                project.teams_assigned,
                project.design.name,
                crate::team::effective_work_rate(project.teams_assigned),
            )));
        }
    }
    for project in &company.rocket_projects {
        if project.teams_assigned > 0 {
            lines.push(Line::from(format!(
                "    {} eng. team(s) on \"{}\"  (rate: {:.2}/day)",
                project.teams_assigned,
                project.design.name,
                crate::team::effective_work_rate(project.teams_assigned),
            )));
        }
    }
    for order in &company.manufacturing.orders {
        if order.teams_assigned > 0 {
            lines.push(Line::from(format!(
                "    {} mfg. team(s) on {} \"{}\"  (rate: {:.2}/day)",
                order.teams_assigned,
                order.type_label(),
                order.display_name(),
                crate::team::manufacturing_work_rate(order.teams_assigned),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(
        Span::styled("  [H] Hire eng. team ($150K)  [M] Hire mfg. team ($900K)", Style::default().fg(Color::Cyan))
    ));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Teams ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_engines_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let mut lines = vec![
        Line::from(format!("  Engine Projects ({})", company.engine_projects.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];

    if company.engine_projects.is_empty() {
        lines.push(Line::from("  No engine projects yet. Press [N] to start a new design."));
    }

    for (i, project) in company.engine_projects.iter().enumerate() {
        let selected = i == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = match &project.status {
            EngineDesignStatus::InDesign { work_completed, work_required } =>
                format!("In Design [{:.0}/{:.0}]", work_completed, work_required),
            EngineDesignStatus::Testing { work_completed } =>
                format!("Testing [{:.0}] {}", work_completed, project.testing_level()),
            EngineDesignStatus::Revising { remaining_indices, work_completed } =>
                format!("Revising {} flaw(s) [{:.0}/30]", remaining_indices.len(), work_completed),
        };

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(
            format!(
                "  {} {} (Rev {}){:>20}",
                marker, project.design.name, project.revision, status_str,
            ),
            style,
        )));

        // Show details for selected project
        if selected {
            let cycle_name = match project.design.cycle {
                EngineCycle::PressureFed => "Pressure Fed",
                EngineCycle::GasGenerator => "Gas Generator",
                EngineCycle::Expander => "Expander",
                EngineCycle::StagedCombustion => "Staged Combustion",
                EngineCycle::FullFlow => "Full Flow",
            };

            // Propellant display with 2 sig figs
            let prop_str: Vec<String> = project.design.propellant_mix.iter()
                .map(|f| format!("{} {:.0}%", f.propellant.display_name(), f.mass_fraction * 100.0))
                .collect();

            lines.push(Line::from(format!(
                "      {}  {}  {:.0}kN  {:.0}s",
                cycle_name,
                prop_str.join(" / "),
                project.design.thrust_n / 1000.0,
                project.design.isp_s,
            )));
            lines.push(Line::from(format!(
                "      Mass: {:.0} kg    Teams: {}    Scale: {:.2}x",
                project.design.mass_kg,
                project.teams_assigned,
                project.scale,
            )));

            // Show flaws if any discovered
            let discovered = project.discovered_flaw_count();
            if discovered > 0 {
                lines.push(Line::from(format!(
                    "      Flaws: {} discovered",
                    discovered,
                )));
                for flaw in &project.flaws {
                    if flaw.discovered {
                        let consequence_str = match &flaw.consequence {
                            FlawConsequence::PerformanceDegradation(frac) =>
                                format!("{:.0}% perf loss", frac * 100.0),
                            FlawConsequence::EngineLoss => "engine loss".to_string(),
                            FlawConsequence::StageLoss => "stage loss".to_string(),
                        };
                        lines.push(Line::from(Span::styled(
                            format!(
                                "        ⚠ {}: {} ({:.0}%/flight)",
                                flaw.description, consequence_str, flaw.activation_chance * 100.0,
                            ),
                            Style::default().fg(Color::Red),
                        )));
                    }
                }
            }
        }
    }

    // Contracted engines section
    if !company.contracted_engines.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("  Contracted Engines"));
        lines.push(Line::from("  ─────────────────────────────────────────────"));
        for ce in &company.contracted_engines {
            lines.push(Line::from(format!(
                "    {} [3P]  {:.0}kN  {:.0}s  {}/unit",
                ce.design.name,
                ce.design.thrust_n / 1000.0,
                ce.design.isp_s,
                format_money(ce.purchase_cost_per_unit),
            )));
        }
    }

    lines.push(Line::from(""));
    let mut controls = vec!["[N] New design", "[B] Contract 3rd-party"];
    if !company.engine_projects.is_empty() {
        controls.extend_from_slice(&["[+] Add team", "[-] Remove team", "[R] Revise"]);
    }
    lines.push(Line::from(Span::styled(
        format!("  {}", controls.join("  ")),
        Style::default().fg(Color::Cyan),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Engines ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_rockets_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let mut lines = vec![
        Line::from(format!("  Rocket Projects ({})", company.rocket_projects.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];

    if company.rocket_projects.is_empty() {
        lines.push(Line::from("  No rocket projects yet. Press [N] to start a new design."));
    }

    for (i, project) in company.rocket_projects.iter().enumerate() {
        let selected = i == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = match &project.status {
            crate::rocket_project::RocketDesignStatus::InDesign { work_completed, work_required } =>
                format!("In Design [{:.0}/{:.0}]", work_completed, work_required),
            crate::rocket_project::RocketDesignStatus::Testing { work_completed } =>
                format!("Testing [{:.0}] {}", work_completed, project.testing_level()),
            crate::rocket_project::RocketDesignStatus::Revising { remaining_indices, work_completed } =>
                format!("Revising {} flaw(s) [{:.0}/30]", remaining_indices.len(), work_completed),
        };

        let total_stages: u32 = project.design.stage_groups.iter()
            .map(|g| g.len() as u32).sum();
        let total_engines: u32 = project.design.stage_groups.iter()
            .flat_map(|g| g.iter())
            .map(|s| s.engine_count)
            .sum();

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(
            format!(
                "  {} {} (Rev {})  {}",
                marker, project.design.name, project.revision, status_str,
            ),
            style,
        )));

        if selected {
            lines.push(Line::from(format!(
                "      {} stages, {} engines    Teams: {}    Complexity: {}",
                total_stages, total_engines, project.teams_assigned, project.complexity,
            )));
            lines.push(Line::from(format!(
                "      Total mass: {:.0} kg    dV: {:.0} m/s (0 payload)",
                project.design.total_mass_kg(),
                project.design.total_delta_v(0.0),
            )));

            // Show payload table
            let table = crate::rocket_project::payload_table(&project.design, "earth_surface");
            if !table.is_empty() {
                lines.push(Line::from("      Max payload:"));
                for (dest, payload) in &table {
                    lines.push(Line::from(format!(
                        "        {:20} {:>8}", dest, format_mass(*payload),
                    )));
                }
            }

            // Show flaws
            let discovered = project.discovered_flaw_count();
            if discovered > 0 {
                lines.push(Line::from(format!("      Flaws: {} discovered", discovered)));
                for flaw in &project.flaws {
                    if flaw.discovered {
                        let consequence_str = match &flaw.consequence {
                            crate::flaw::FlawConsequence::PerformanceDegradation(frac) =>
                                format!("{:.0}% perf loss", frac * 100.0),
                            crate::flaw::FlawConsequence::EngineLoss => "engine loss".to_string(),
                            crate::flaw::FlawConsequence::StageLoss => "stage loss".to_string(),
                        };
                        lines.push(Line::from(Span::styled(
                            format!(
                                "        ⚠ {}: {} ({:.0}%/flight)",
                                flaw.description, consequence_str, flaw.activation_chance * 100.0,
                            ),
                            Style::default().fg(Color::Red),
                        )));
                    }
                }
            }

            // Inventory count
            let built = company.manufacturing.inventory.rocket_count(project.project_id);
            if built > 0 {
                lines.push(Line::from(format!("      Built rockets: {}", built)));
            }
        }
    }

    lines.push(Line::from(""));
    let mut controls = vec!["[N] New design"];
    if !company.rocket_projects.is_empty() {
        controls.extend_from_slice(&[
            "[+] Add team", "[-] Remove team",
            "[R] Revise", "[O] Order build",
        ]);
    }
    lines.push(Line::from(Span::styled(
        format!("  {}", controls.join("  ")),
        Style::default().fg(Color::Cyan),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Rockets ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_manufacturing_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let mfg = &company.manufacturing;
    let mut lines = vec![
        Line::from("  Manufacturing"),
        Line::from("  ─────────────────────────────────────────────"),
        Line::from(format!(
            "  Floor space: {}/{} used    Mfg teams: {} ({} unassigned)",
            mfg.floor_space_in_use(),
            mfg.floor_space.total_units,
            company.manufacturing_teams.len(),
            company.unassigned_manufacturing_team_count(),
        )),
    ];

    // Show floor space construction
    for order in &mfg.floor_space.under_construction {
        lines.push(Line::from(format!(
            "    Building {} unit(s): {} days remaining",
            order.units, order.days_remaining,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("  Orders:"));

    if mfg.orders.is_empty() {
        lines.push(Line::from("    No manufacturing orders."));
    }

    for (i, order) in mfg.orders.iter().enumerate() {
        let selected = i == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = if order.waiting_for_prerequisites {
            "Waiting".to_string()
        } else {
            format!("[{:.0}/{:.0}]  Teams: {}",
                order.work_completed, order.work_required, order.teams_assigned)
        };

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(
            format!(
                "    {} [{}] {} \"{}\"  {}",
                marker, i + 1, order.type_label(), order.display_name(), status_str,
            ),
            style,
        )));
    }

    // Inventory summary
    lines.push(Line::from(""));
    lines.push(Line::from("  Inventory:"));
    if mfg.inventory.engines.is_empty() && mfg.inventory.stages.is_empty() && mfg.inventory.rockets.is_empty() {
        lines.push(Line::from("    (empty)"));
    } else {
        if !mfg.inventory.engines.is_empty() {
            // Group engines by name
            let mut engine_counts: Vec<(&str, usize)> = Vec::new();
            for eng in &mfg.inventory.engines {
                if let Some(entry) = engine_counts.iter_mut().find(|(n, _)| *n == eng.engine_name.as_str()) {
                    entry.1 += 1;
                } else {
                    engine_counts.push((&eng.engine_name, 1));
                }
            }
            for (name, count) in &engine_counts {
                lines.push(Line::from(format!("    {} engines: {}", name, count)));
            }
        }
        if !mfg.inventory.stages.is_empty() {
            lines.push(Line::from(format!("    Stages: {}", mfg.inventory.stages.len())));
        }
        if !mfg.inventory.rockets.is_empty() {
            for rocket_inv in &mfg.inventory.rockets {
                lines.push(Line::from(format!("    Rocket: {}", rocket_inv.rocket_name)));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [B] Buy floor space ($5M)  [+] Add mfg team  [-] Remove mfg team",
        Style::default().fg(Color::Cyan),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Manufacturing ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_contracts_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let game = &app.game;
    let available = &game.available_contracts;
    let accepted = &game.player_company.active_contracts;
    let rep = game.player_company.reputation.total();

    let mut lines = vec![
        Line::from(Span::styled(
            format!("  Reputation: {:.0}", rep),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
    ];

    // Available contracts section
    lines.push(Line::from(Span::styled(
        "  ── Available Contracts ──",
        Style::default().fg(Color::DarkGray),
    )));

    if available.is_empty() {
        lines.push(Line::from("  (none available — wait for next month)"));
    } else {
        for (i, c) in available.iter().enumerate() {
            let marker = if i == app.selected_item { "▶ " } else { "  " };
            let dest_name = contract::destination_display_name(&c.destination);
            let style = if i == app.selected_item {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}  →{}  {:.0} kg  {}  by {}",
                    marker, c.name, dest_name,
                    c.payload_kg, format_money(c.payment), c.deadline),
                style,
            )));
        }
    }

    lines.push(Line::from(""));

    // Accepted contracts section
    lines.push(Line::from(Span::styled(
        "  ── Accepted Contracts ──",
        Style::default().fg(Color::DarkGray),
    )));

    if accepted.is_empty() {
        lines.push(Line::from("  (none accepted)"));
    } else {
        let offset = available.len();
        for (i, c) in accepted.iter().enumerate() {
            let idx = offset + i;
            let marker = if idx == app.selected_item { "▶ " } else { "  " };
            let dest_name = contract::destination_display_name(&c.destination);
            let style = if idx == app.selected_item {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}  →{}  {:.0} kg  {}  by {}",
                    marker, c.name, dest_name,
                    c.payload_kg, format_money(c.payment), c.deadline),
                style,
            )));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Contracts  [A] Accept ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_launches_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let game = &app.game;
    let rockets = &game.player_company.manufacturing.inventory.rockets;

    let mut lines = vec![];

    // Show inventory rockets ready for launch
    lines.push(Line::from(Span::styled(
        "  ── Ready Rockets ──",
        Style::default().fg(Color::DarkGray),
    )));

    if rockets.is_empty() {
        lines.push(Line::from("  (no rockets in inventory)"));
    } else {
        for (i, r) in rockets.iter().enumerate() {
            let marker = if i == app.selected_item { "▶ " } else { "  " };
            let style = if i == app.selected_item {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            // Find the design to show payload capacity
            let payload_info = game.player_company.rocket_projects.iter()
                .find(|rp| rp.project_id == r.rocket_project_id)
                .map(|rp| {
                    let leo = rocket_project::max_payload_to(&rp.design, "earth_surface", "leo");
                    format!("  LEO: {}", format_mass(leo))
                })
                .unwrap_or_default();

            lines.push(Line::from(Span::styled(
                format!("{}{}{}", marker, r.rocket_name, payload_info),
                style,
            )));
        }
    }

    lines.push(Line::from(""));

    // Recent launch history
    lines.push(Line::from(Span::styled(
        "  ── Launch History ──",
        Style::default().fg(Color::DarkGray),
    )));

    let history = &game.player_company.launch_history;
    if history.is_empty() {
        lines.push(Line::from("  (no launches yet)"));
    } else {
        for record in history.iter().rev().take(15) {
            let dest_name = contract::destination_display_name(&record.destination);
            let outcome_str = match &record.outcome {
                LaunchOutcome::Success => Span::styled("SUCCESS", Style::default().fg(Color::Green)),
                LaunchOutcome::PartialFailure { .. } => Span::styled("PARTIAL", Style::default().fg(Color::Yellow)),
                LaunchOutcome::Failure { .. } => Span::styled("FAILURE", Style::default().fg(Color::Red)),
            };
            lines.push(Line::from(vec![
                Span::raw(format!("  {} {} →{} ", record.launch_date, record.rocket_name, dest_name)),
                outcome_str,
            ]));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Launches  [L] Launch ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_finance_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let game = &app.game;
    let company = &game.player_company;
    let financials = &company.monthly_financials;

    let salary = company.monthly_salary_cost();
    let runway = if salary > 0.0 && company.money > 0.0 {
        format!("{:.0} months", company.money / salary)
    } else if salary <= 0.0 {
        "∞".to_string()
    } else {
        "0 months".to_string()
    };

    let mut lines = vec![
        Line::from(format!("  Balance: {}", format_money(company.money))),
        Line::from(format!("  Monthly Salary: {}", format_money(salary))),
        Line::from(format!("  Runway: {}", runway)),
        Line::from(format!("  Reputation: {:.0}", company.reputation.total())),
        Line::from(""),
    ];

    // Reputation breakdown — only show non-zero factors
    let rep = &company.reputation;
    let factors: Vec<(&str, f64)> = vec![
        ("Success", rep.success_factor),
        ("Lost Payload", rep.lost_payload_factor),
        ("Drought", rep.drought_factor),
        ("Expiry", rep.expiry_factor),
    ];
    let active_factors: Vec<_> = factors.iter().filter(|(_, v)| v.abs() > 0.05).collect();
    if !active_factors.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ── Reputation Factors ──",
            Style::default().fg(Color::DarkGray),
        )));
        for &(name, value) in &active_factors {
            let color = if *value > 0.0 { Color::Green } else { Color::Red };
            lines.push(Line::from(Span::styled(
                format!("  {:<14} {:+.1}", name, value),
                Style::default().fg(color),
            )));
        }
        lines.push(Line::from(""));
    }

    // Monthly financials
    lines.push(Line::from(Span::styled(
        "  ── Monthly Financials ──",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from("  Month         Income       Expenses     Net"));
    lines.push(Line::from("  ─────────────────────────────────────────────"));

    if financials.is_empty() {
        lines.push(Line::from("  (no data yet)"));
    } else {
        for f in financials.iter().rev() {
            let net = f.income - f.expenses;
            let net_style = if net >= 0.0 { Color::Green } else { Color::Red };
            let month_name = crate::calendar::GameDate::new(f.year, f.month, 1);
            lines.push(Line::from(vec![
                Span::raw(format!("  {:<14} {:>12} {:>12} ",
                    format!("{}", month_name.month_name()),
                    format_money(f.income),
                    format_money(f.expenses),
                )),
                Span::styled(format!("{:>12}", format_money_signed(net)), Style::default().fg(net_style)),
            ]));
        }
    }

    // Rocket Costs section
    if !company.rocket_projects.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ── Rocket Costs ──",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from("  Design            NRE          Avg Cost     Marginal     Built"));
        lines.push(Line::from("  ─────────────────────────────────────────────────────────────────"));

        for rp in &company.rocket_projects {
            let design_id = rp.design.id;

            // Compute NRE: rocket project NRE + apportioned engine NRE
            let mut total_nre = rp.nre_cost;
            for group in &rp.design.stage_groups {
                for stage in group {
                    // Find the engine project for this engine
                    if let Some(ep) = company.engine_projects.iter()
                        .find(|ep| ep.design.id == stage.engine.id)
                    {
                        // Apportion: (engines used by this design / total engines built) * engine NRE
                        let total_built = *company.engine_build_counts
                            .get(&ep.project_id)
                            .unwrap_or(&0);
                        if total_built > 0 {
                            let engines_in_design = stage.engine_count as f64;
                            let fraction = engines_in_design / total_built as f64;
                            total_nre += fraction * ep.nre_cost;
                        }
                    }
                }
            }

            let cost_history = company.rocket_cost_history.get(&design_id);
            let built = cost_history.map_or(0, |h| h.len());

            let (avg_str, marginal_str) = if built > 0 {
                let history = cost_history.unwrap();
                let total_build: f64 = history.iter().sum();
                let avg = (total_nre + total_build) / built as f64;
                let marginal = *history.last().unwrap();
                (format_money(avg), format_money(marginal))
            } else {
                ("—".to_string(), "—".to_string())
            };

            let name = if rp.design.name.len() > 18 {
                format!("{}…", &rp.design.name[..17])
            } else {
                rp.design.name.clone()
            };

            lines.push(Line::from(format!(
                "  {:<18} {:>12} {:>12} {:>12} {:>5}",
                name, format_money(total_nre), avg_str, marginal_str, built
            )));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Finance ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_events_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let inner_height = area.height.saturating_sub(2) as usize; // minus borders
    let recent = app.game.event_log.recent(inner_height + app.content_scroll);

    let items: Vec<ListItem> = recent.iter()
        .skip(app.content_scroll)
        .take(inner_height)
        .map(|(date, event)| {
            let style = match event.importance() {
                EventImportance::Notable => Style::default().fg(Color::White),
                EventImportance::Routine => Style::default().fg(Color::DarkGray),
            };
            ListItem::new(format!("  {}: {}", date, event)).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Events ");
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_event_feed(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let recent = app.game.event_log.recent(inner_height);

    let items: Vec<ListItem> = recent.iter().map(|(date, event)| {
        let style = match event.importance() {
            EventImportance::Notable => Style::default().fg(Color::Cyan),
            EventImportance::Routine => Style::default().fg(Color::DarkGray),
        };
        ListItem::new(format!("  {}: {}", date, event)).style(style)
    }).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Recent ");
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let text = if let Some(ref msg) = app.status_message {
        format!(" {} ", msg)
    } else if !matches!(app.input_mode, InputMode::Normal) {
        " [Enter] Confirm  [Esc] Cancel  [↑↓] Select ".to_string()
    } else {
        " [Space] Pause/Unpause  [1-3] Speed  [←→] Pane  [↑↓] Select  [S] Save  [Q] Quit ".to_string()
    };
    let style = if app.status_message.is_some() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default().borders(Borders::ALL);
    let paragraph = Paragraph::new(text).block(block).style(style);
    frame.render_widget(paragraph, area);
}

// ==========================================
// Rocket Designer — Full Screen
// ==========================================

fn draw_rocket_designer_full(frame: &mut Frame, app: &App, state: &RocketDesignerState, area: Rect) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),     // designer content
            Constraint::Length(3),    // help bar
        ])
        .split(area);

    draw_rocket_designer_content(frame, state, outer[0]);

    // Help bar for designer
    let help_text = if let Some(ref msg) = app.status_message {
        format!(" {} ", msg)
    } else {
        " [Enter] Edit  [←→] Engines  [+/-] Prop  [A] Add  [I] Ins  [B] Booster  [X] Rem  [P] Payload  [L] Site  [D] Done  [Esc] Cancel ".to_string()
    };
    let style = if app.status_message.is_some() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default().borders(Borders::ALL);
    let paragraph = Paragraph::new(help_text).block(block).style(style);
    frame.render_widget(paragraph, outer[1]);
}

fn draw_rocket_designer_content(frame: &mut Frame, state: &RocketDesignerState, area: Rect) {
    let mut lines = Vec::new();

    // Launch site display name
    let launch_display = DELTA_V_MAP.location(state.launch_from)
        .map_or(state.launch_from, |l| l.display_name);

    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        "  Launch: {}    Payload: {:.0} kg",
        launch_display, state.payload_kg,
    )));
    lines.push(Line::from(""));

    // Build a temporary RocketDesign to compute stats
    let temp_design = rocket::RocketDesign {
        id: rocket::RocketDesignId(0),
        name: state.rocket_name.clone(),
        stage_groups: state.stage_groups.clone(),
    };

    let stats = if !state.stage_groups.is_empty() {
        rocket::compute_stage_stats(&temp_design, state.payload_kg, state.launch_from)
    } else {
        Vec::new()
    };

    lines.push(Line::from(Span::styled(
        format!(
            "   #   {:<14} {:>2}  {:>7} {:>6}  {:>5}  {:>6}  {:>8}  {:>5}",
            "Engine", " N", "Prop", "Burn", "MR", "Eff dV", "Vac dV", "TWR",
        ),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(
        "  ─────────────────────────────────────────────────────────────────────"
    ));

    // Stage rows
    for (gi, group) in state.stage_groups.iter().enumerate() {
        let group_len = group.len();
        // Indentation prefix for multi-stage groups (boosters)
        let group_indent = if group_len > 1 { "  " } else { "" };

        for (si, stage) in group.iter().enumerate() {
            let selected = gi == state.selected_group && si == state.selected_inner;
            let marker = if selected { "▶" } else { " " };

            let tag = match state.engine_sources.get(gi).and_then(|g| g.get(si)) {
                Some(EngineSource::Contracted(_)) => "[3P]",
                _ => "",
            };
            let engine_label = format!("{}{}", stage.engine.name, tag);

            // Compute burn time: propellant_mass / (mass_flow_rate * engine_count)
            let burn_time_s = {
                let mfr = stage.engine.mass_flow_rate() * stage.engine_count as f64;
                if mfr > 0.0 { stage.propellant_mass_kg / mfr } else { 0.0 }
            };

            // Stats only shown on the last inner stage of a group
            let is_last_in_group = si + 1 == group_len;
            let stat_str = if is_last_in_group {
                if let Some(s) = stats.get(gi) {
                    format!(
                        "{:>5.0}s  {:>5.1}  {:>6.0}  {:>8.0}  {:>5.2}",
                        burn_time_s,
                        s.mass_ratio,
                        s.delta_v_effective,
                        s.delta_v_vacuum,
                        s.twr,
                    )
                } else {
                    format!("{:>5.0}s", burn_time_s)
                }
            } else {
                format!("{:>5.0}s", burn_time_s)
            };

            let style = if selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            // Stage name: S1 for single-stage groups, S1a/S1b for multi-stage
            let stage_label = RocketDesignerState::stage_name(gi, si, group_len);
            // Indent inner stages (not first in multi-stage group),
            // but keep total width of indent+label constant at 4 chars
            let label_col = if group_len > 1 && si > 0 {
                format!("  {:<2}", stage_label)
            } else {
                format!("{:<4}", stage_label)
            };

            lines.push(Line::from(Span::styled(
                format!(
                    " {} {} {:<14} x{}  {:>7}  {}",
                    marker,
                    label_col,
                    engine_label,
                    stage.engine_count,
                    format_mass(stage.propellant_mass_kg),
                    stat_str,
                ),
                style,
            )));

            // Show losses sub-line after the last inner stage of a group
            if is_last_in_group {
                if let Some(s) = stats.get(gi) {
                    let mut loss_parts = Vec::new();
                    if s.gravity_loss >= 0.5 {
                        loss_parts.push(format!("grav: -{:.0}", s.gravity_loss));
                    }
                    if s.aero_drag_loss >= 0.5 {
                        loss_parts.push(format!("aero: -{:.0}", s.aero_drag_loss));
                    }
                    if !loss_parts.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("{}                 ({})", group_indent, loss_parts.join("  ")),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }
        }
    }

    // "Add stage" slot
    let on_add = state.on_add_slot();
    let add_marker = if on_add { "▶" } else { " " };
    let add_style = if on_add {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    lines.push(Line::from(Span::styled(
        format!(" {} ── add stage ──", add_marker),
        add_style,
    )));

    lines.push(Line::from(""));

    // Totals
    if !stats.is_empty() {
        let total_dv_effective: f64 = stats.iter().map(|s| s.delta_v_effective).sum();
        let total_dv_vacuum: f64 = stats.iter().map(|s| s.delta_v_vacuum).sum();
        let total_mass = temp_design.total_mass_kg() + state.payload_kg;

        lines.push(Line::from(format!(
            "  Total dV: {:.0} m/s (vacuum: {:.0})",
            total_dv_effective, total_dv_vacuum,
        )));
        lines.push(Line::from(format!(
            "  Total mass: {}",
            format_mass(total_mass),
        )));
        lines.push(Line::from(""));

        // Payload feasibility table
        let table = rocket_project::payload_table(&temp_design, state.launch_from);
        if !table.is_empty() {
            lines.push(Line::from(Span::styled(
                "  Payload Feasibility:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for (dest, payload) in &table {
                lines.push(Line::from(format!(
                    "    {:24} {:>8}", dest, format_mass(*payload),
                )));
            }
        }
    }

    let title = format!(" Rocket Designer: \"{}\" ", state.rocket_name);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(Style::default().fg(Color::Yellow));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

// ==========================================
// Modal overlays (engine design flow + rocket sub-modals)
// ==========================================

fn draw_modal(frame: &mut Frame, app: &App, area: Rect) {
    let modal_area = centered_rect(60, 50, area);
    frame.render_widget(Clear, modal_area);

    match &app.input_mode {
        InputMode::Normal | InputMode::RocketDesigner { .. } => {}
        InputMode::EngineName { buffer } => {
            let lines = vec![
                Line::from(""),
                Line::from("  Enter engine name:"),
                Line::from(""),
                Line::from(format!("  > {}█", buffer)),
            ];
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" New Engine Design ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::SelectCycle { name, selected } => {
            let cycles = [
                ("Pressure Fed", "Simple, reliable, lower performance"),
                ("Gas Generator", "Good all-around, moderate complexity"),
                ("Expander", "Efficient, limited thrust"),
                ("Staged Combustion", "High performance, complex"),
                ("Full Flow", "Maximum performance, most complex"),
                ("Solid Rocket Motor", "Simple, cheap, not throttleable"),
            ];
            let mut lines = vec![
                Line::from(format!("  Design: {}", name)),
                Line::from(""),
                Line::from("  Select cycle type:"),
                Line::from(""),
            ];
            for (i, (name, desc)) in cycles.iter().enumerate() {
                let marker = if i == *selected { "▶" } else { " " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {} {}  — {}", marker, name, desc),
                    style,
                )));
            }
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Select Cycle ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::SelectPropellant { name, cycle, selected } => {
            let presets: Vec<PropellantPreset> = PropellantPreset::ALL.iter()
                .filter(|p| p.compatible_cycles().contains(cycle))
                .copied()
                .collect();
            let mut lines = vec![
                Line::from(format!("  Design: {}  Cycle: {:?}", name, cycle)),
                Line::from(""),
                Line::from("  Select propellant:"),
                Line::from(""),
            ];
            for (i, preset) in presets.iter().enumerate() {
                let marker = if i == *selected { "▶" } else { " " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {} {}", marker, preset.name()),
                    style,
                )));
            }
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Select Propellant ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::SelectScale { name, cycle, preset, scale, use_vacuum } => {
            let baseline = engine_project::engine_baseline(*cycle, *preset);
            let mut lines = vec![
                Line::from(format!("  Design: {}  {:?}  {}", name, cycle, preset.name())),
                Line::from(""),
            ];
            if let Some(b) = baseline {
                let thrust = b.thrust_n * scale;
                let mass = b.mass_kg * scale;
                let isp = if *use_vacuum { b.isp_vac_s } else { b.isp_sl_s };
                lines.push(Line::from(format!("  Scale: {:.2}x  [↑↓ to adjust]", scale)));
                lines.push(Line::from(""));
                lines.push(Line::from(format!("  Thrust: {:.0} kN", thrust / 1000.0)));
                lines.push(Line::from(format!("  Mass:   {:.0} kg", mass)));
                lines.push(Line::from(format!(
                    "  Isp:    {:.0} s ({})",
                    isp,
                    if *use_vacuum { "vacuum" } else { "sea level" },
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  [V] Toggle vacuum/sea-level  [Enter] Confirm",
                    Style::default().fg(Color::Cyan),
                )));
            }
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Set Scale ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::SelectThirdParty { selected } => {
            let catalog = &app.game.player_company.third_party_catalog;
            let mut lines = vec![
                Line::from("  Available third-party engines:"),
                Line::from(""),
            ];
            for (i, entry) in catalog.iter().enumerate() {
                let marker = if i == *selected { "▶" } else { " " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
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
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Contract Third-Party Engine ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::RocketName { buffer } => {
            let lines = vec![
                Line::from(""),
                Line::from("  Enter rocket name:"),
                Line::from(""),
                Line::from(format!("  > {}█", buffer)),
            ];
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" New Rocket Design ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::RocketPickEngine { state, selected, .. } => {
            draw_rocket_pick_engine_modal(frame, app, state, *selected, modal_area);
        }
        InputMode::RocketPayloadInput { buffer, .. } => {
            let lines = vec![
                Line::from(""),
                Line::from("  Enter payload mass (kg):"),
                Line::from(""),
                Line::from(format!("  > {}█", buffer)),
            ];
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Set Payload ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::LaunchSelectContract { selected, .. } => {
            let contracts = &app.game.player_company.active_contracts;
            let total_options = contracts.len() + 1;
            let mut lines = vec![
                Line::from(""),
                Line::from("  Select mission:"),
                Line::from(""),
            ];
            for (i, c) in contracts.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let dest_name = contract::destination_display_name(&c.destination);
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("{}{} → {} ({:.0} kg, {})",
                        marker, c.name, dest_name, c.payload_kg, format_money(c.payment)),
                    style,
                )));
            }
            // Test launch option
            let test_idx = contracts.len();
            let marker = if test_idx == *selected { " ▶ " } else { "   " };
            let style = if test_idx == *selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(Span::styled(
                format!("{}Test Launch (LEO, no payload)", marker),
                style,
            )));
            let _ = total_options; // suppress unused warning
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Launch Mission ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
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

            lines.push(Line::from(Span::styled(
                "  Press any key to continue",
                Style::default().fg(Color::DarkGray),
            )));

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Launch Result ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
    }
}

fn draw_rocket_pick_engine_modal(
    frame: &mut Frame,
    app: &App,
    state: &RocketDesignerState,
    selected: usize,
    area: Rect,
) {
    let engines = app.available_engines();

    let mut lines = vec![
        Line::from(format!("  Rocket: {}    Stages: {}", state.rocket_name, state.stage_groups.len())),
        Line::from(""),
        Line::from("  Select engine:"),
        Line::from(""),
    ];

    if engines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No engines ready! Design and test an engine first, or contract a 3rd-party engine.",
            Style::default().fg(Color::Red),
        )));
    }

    for (i, (source, design)) in engines.iter().enumerate() {
        let marker = if i == selected { "▶" } else { " " };
        let tag = if matches!(source, EngineSource::Contracted(_)) { " [3P]" } else { "" };
        let style = if i == selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("  {} {}{}  {:.0}kN  {:.0}s  {:.0}kg",
                marker, design.name, tag, design.thrust_n / 1000.0, design.isp_s, design.mass_kg),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Enter] Select  [Esc] Back",
        Style::default().fg(Color::Cyan),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pick Engine ")
        .style(Style::default().fg(Color::Yellow));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Format a mass in kg, switching to tons if >= 1000 kg.
fn format_mass(kg: f64) -> String {
    if kg >= 1000.0 {
        format!("{:.1}t", kg / 1000.0)
    } else {
        format!("{:.0}kg", kg)
    }
}

fn format_money_signed(amount: f64) -> String {
    if amount >= 0.0 {
        format!("+{}", format_money(amount))
    } else {
        format_money(amount)
    }
}

pub fn format_money(amount: f64) -> String {
    crate::resources::format_money(amount)
}
