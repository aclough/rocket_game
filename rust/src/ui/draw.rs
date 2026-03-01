use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::engine::EngineCycle;
use crate::engine_project::{self, EngineDesignStatus, EngineProjectId, PropellantPreset};
use crate::event::EventImportance;
use crate::flaw::FlawConsequence;
use crate::structure;
use crate::ui::{App, FocusedPane, InputMode, Tab};

/// Draw the entire application frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

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

        let third_party_tag = if project.is_third_party { " [3P]" } else { "" };

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(
            format!(
                "  {} {} (Rev {}){:>20}{}",
                marker, project.design.name, project.revision, status_str, third_party_tag,
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
            if matches!(project.status, EngineDesignStatus::Testing { .. }) {
                lines.push(Line::from(Span::styled(
                    "        ? Unknown flaws may remain",
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    lines.push(Line::from(""));
    let mut controls = vec!["[N] New design", "[B] Buy 3rd-party"];
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
        lines.push(Line::from("  No rocket projects yet."));
        lines.push(Line::from("  (Rocket designer coming soon — projects can be"));
        lines.push(Line::from("   created via the game state for now.)"));
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
                        "        {:20} {:.0} kg", dest, payload,
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
            if matches!(project.status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
                lines.push(Line::from(Span::styled(
                    "        ? Unknown flaws may remain",
                    Style::default().fg(Color::DarkGray),
                )));
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
            for rocket in &mfg.inventory.rockets {
                lines.push(Line::from(format!("    Rocket: {}", rocket.rocket_name)));
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

fn draw_modal(frame: &mut Frame, app: &App, area: Rect) {
    let modal_area = centered_rect(60, 50, area);
    frame.render_widget(Clear, modal_area);

    match &app.input_mode {
        InputMode::Normal => {}
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
                let d = &entry.project.design;
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {} {}  {:.0}kN  {:.0}s  {}",
                        marker,
                        d.name,
                        d.thrust_n / 1000.0,
                        d.isp_s,
                        format_money(entry.purchase_cost),
                    ),
                    style,
                )));
            }
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Buy Third-Party Engine ")
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
        InputMode::RocketSelectEngine { rocket_name, stage_groups, selected, .. } => {
            let engines: Vec<(EngineProjectId, &crate::engine::EngineDesign)> =
                app.game.player_company.engine_projects.iter()
                    .filter(|ep| matches!(ep.status, EngineDesignStatus::Testing { .. }))
                    .map(|ep| (ep.project_id, &ep.design))
                    .collect();

            let total_stages: usize = stage_groups.iter().map(|g| g.len()).sum();
            let mut lines = vec![
                Line::from(format!("  Rocket: {}    Stages added: {}", rocket_name, total_stages)),
                Line::from(""),
            ];

            // Show existing stages
            for (gi, group) in stage_groups.iter().enumerate() {
                for stage in group {
                    lines.push(Line::from(format!(
                        "    Stage {}: {} x{}  {:.0}kg prop  {:.0}kg struct",
                        gi + 1, stage.engine.name, stage.engine_count,
                        stage.propellant_mass_kg, stage.structural_mass_kg,
                    )));
                }
            }

            if !stage_groups.is_empty() {
                lines.push(Line::from(""));
            }

            lines.push(Line::from("  Select engine for next stage:"));
            lines.push(Line::from(""));

            if engines.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No engines ready! Design and test an engine first.",
                    Style::default().fg(Color::Red),
                )));
            }

            for (i, (_ep_id, design)) in engines.iter().enumerate() {
                let marker = if i == *selected { "▶" } else { " " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {} {}  {:.0}kN  {:.0}s  {:.0}kg",
                        marker, design.name, design.thrust_n / 1000.0, design.isp_s, design.mass_kg),
                    style,
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] Select  [D] Done  [Esc] Cancel",
                Style::default().fg(Color::Cyan),
            )));

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Add Stage ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::RocketConfigStage {
            rocket_name, stage_groups, engine, engine_count, propellant_mass_kg, ..
        } => {
            let group_index = stage_groups.len();
            let is_first = group_index == 0;
            let propellant_mix: Vec<(crate::propellant::Propellant, f64)> =
                engine.propellant_mix.iter()
                    .map(|f| (f.propellant, f.mass_fraction))
                    .collect();
            let breakdown = structure::compute_structural_mass(
                *propellant_mass_kg,
                &propellant_mix,
                engine,
                *engine_count,
                is_first,
                true,
            );

            let lines = vec![
                Line::from(format!("  Rocket: {}    Stage {}", rocket_name, group_index + 1)),
                Line::from(""),
                Line::from(format!("  Engine: {}  ({:.0}kN, {:.0}s)", engine.name, engine.thrust_n / 1000.0, engine.isp_s)),
                Line::from(""),
                Line::from(format!("  Engine count:   {}    [←→ to adjust]", engine_count)),
                Line::from(format!("  Propellant:     {:.0} kg  [↑↓ to adjust, 5t steps]", propellant_mass_kg)),
                Line::from(""),
                Line::from(format!("  Structural mass: {:.0} kg", breakdown.total)),
                Line::from(format!("    Tank: {:.0}  Thrust struct: {:.0}  Aero: {:.0}  Interstage: {:.0}",
                    breakdown.tank_mass, breakdown.thrust_structure, breakdown.aero_shell, breakdown.interstage)),
                Line::from(""),
                Line::from(format!("  Total thrust:    {:.0} kN", engine.thrust_n * *engine_count as f64 / 1000.0)),
                Line::from(format!("  Dry mass:        {:.0} kg",
                    breakdown.total + engine.mass_kg * *engine_count as f64)),
                Line::from(format!("  Wet mass:        {:.0} kg",
                    breakdown.total + engine.mass_kg * *engine_count as f64 + propellant_mass_kg)),
                Line::from(""),
                Line::from(Span::styled(
                    "  [Enter] Add stage  [Esc] Back",
                    Style::default().fg(Color::Cyan),
                )),
            ];

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Configure Stage ")
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
    }
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

pub fn format_money(amount: f64) -> String {
    if amount >= 1_000_000_000.0 {
        format!("${:.1}B", amount / 1_000_000_000.0)
    } else if amount >= 1_000_000.0 {
        format!("${:.1}M", amount / 1_000_000.0)
    } else if amount >= 1_000.0 {
        format!("${:.0}K", amount / 1_000.0)
    } else if amount < 0.0 {
        if amount <= -1_000_000_000.0 {
            format!("-${:.1}B", (-amount) / 1_000_000_000.0)
        } else if amount <= -1_000_000.0 {
            format!("-${:.1}M", (-amount) / 1_000_000.0)
        } else if amount <= -1_000.0 {
            format!("-${:.0}K", (-amount) / 1_000.0)
        } else {
            format!("-${:.0}", -amount)
        }
    } else {
        format!("${:.0}", amount)
    }
}
