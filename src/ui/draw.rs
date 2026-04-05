use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph};

use crate::contract::{self, Contract};
use crate::engine::EngineCycle;
use crate::engine_project::{self, EngineDesignStatus, EngineSource, PropellantPreset};
use crate::game_state::Company;
use crate::manufacturing::ManufacturingOrderType;
use crate::rocket_project;
use crate::event::EventImportance;
use crate::flaw::{Flaw, FlawConsequence, FlawTrigger};
use crate::launch::LaunchOutcome;
use crate::location::DELTA_V_MAP;
use crate::rocket;
use crate::ui::{App, FocusedPane, InputMode, RocketDesignerState, Tab};

fn format_flaw_rate(flaw: &Flaw) -> String {
    match flaw.trigger {
        FlawTrigger::PerFlight => format!("{:.0}%/flight", flaw.activation_chance * 100.0),
        FlawTrigger::PerDay => format!("{:.0}%/year, {:.2}%/day", flaw.activation_chance * 100.0, flaw.daily_rate() * 100.0),
    }
}

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
    let econ_pct = ((game.economy.modifier - 1.0) * 100.0).round();
    let econ_str = if econ_pct.abs() < 1.0 {
        String::new()
    } else {
        let sign = if econ_pct > 0.0 { "+" } else { "" };
        format!("      Econ: {}{:.0}%", sign, econ_pct)
    };
    let text = format!(
        "  {}      {}      {}      {}      {}{}",
        game.player_company.name,
        game.date,
        money_str,
        teams_str,
        speed_str,
        econ_str,
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
        {
            let econ = &game.economy;
            let pct = ((econ.modifier - 1.0) * 100.0).round();
            let sign = if pct >= 0.0 { "+" } else { "" };
            let color = if pct > 5.0 { Color::Green }
                else if pct < -20.0 { Color::Rgb(255, 100, 0) }  // orange for recession
                else if pct < -5.0 { Color::Yellow }              // yellow for slowdown
                else { Color::White };
            Line::from(Span::styled(
                format!("  Economy:         {} ({}{}%)", econ.condition.display_name(), sign, pct),
                Style::default().fg(color),
            ))
        },
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

fn draw_engines_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let mut lines = vec![
        Line::from(format!("  Engine Projects ({})", company.engine_projects.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    if company.engine_projects.is_empty() {
        lines.push(Line::from("  No engine projects yet. Press [N] to start a new design."));
    }

    for (i, project) in company.engine_projects.iter().enumerate() {
        let selected = i == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = match &project.status {
            EngineDesignStatus::InDesign { .. } => "In Design".to_string(),
            EngineDesignStatus::Testing { .. } =>
                format!("Testing  {}", project.testing_level()),
            EngineDesignStatus::Revising { remaining_indices, .. } =>
                format!("Revising {} flaw(s)", remaining_indices.len()),
        };

        let line_text = format!(
            "  {} {} (Rev {})  {}",
            marker, project.design.name, project.revision, status_str,
        );
        let text_width = line_text.len() as u16;

        // Track gauge data for this line
        let line_idx = lines.len();
        match &project.status {
            EngineDesignStatus::InDesign { work_completed, work_required } => {
                let ratio = work_completed / work_required;
                gauges.push(GaugeInfo {
                    line_index: line_idx, ratio,
                    label: format!("{:.0}/{:.0}", work_completed, work_required),
                    fill_color: Color::Rgb(0, 140, 140), text_width, right_aligned: false,
                });
            }
            EngineDesignStatus::Testing { work_completed } => {
                let ratio = work_completed / 30.0;
                gauges.push(GaugeInfo {
                    line_index: line_idx, ratio,
                    label: format!("{:.0}/30", work_completed),
                    fill_color: Color::Green, text_width, right_aligned: false,
                });
            }
            EngineDesignStatus::Revising { work_completed, .. } => {
                let ratio = work_completed / 30.0;
                gauges.push(GaugeInfo {
                    line_index: line_idx, ratio,
                    label: format!("{:.0}/30", work_completed),
                    fill_color: Color::Rgb(180, 130, 0), text_width, right_aligned: false,
                });
            }
        }

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(line_text, style)));

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

            // Show inventory count for engines in Testing or later
            if matches!(project.status, EngineDesignStatus::Testing { .. }) {
                let source = EngineSource::PlayerDesign(project.project_id);
                let count = company.manufacturing.inventory.engine_count(source);
                lines.push(Line::from(format!("      Built engines: {}", count)));
            }

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
                                "        ⚠ {}: {} ({})",
                                flaw.description, consequence_str, format_flaw_rate(flaw),
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
            // Show discovered flaws
            for flaw in &ce.flaws {
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

    lines.push(Line::from(""));
    let mut controls = vec!["[N] New design", "[B] Contract 3rd-party"];
    if !company.engine_projects.is_empty() {
        controls.extend_from_slice(&["[+] Add team", "[-] Remove team", "[R] Revise", "[O] Order build", "[E] Hire eng team"]);
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
    render_gauges(frame, area, &gauges);
}

fn draw_rockets_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let mut lines = vec![
        Line::from(format!("  Rocket Projects ({})", company.rocket_projects.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    if company.rocket_projects.is_empty() {
        lines.push(Line::from("  No rocket projects yet. Press [N] to start a new design."));
    }

    for (i, project) in company.rocket_projects.iter().enumerate() {
        let selected = i == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = match &project.status {
            rocket_project::RocketDesignStatus::InDesign { .. } =>
                "In Design".to_string(),
            rocket_project::RocketDesignStatus::Testing { .. } =>
                format!("Testing  {}", project.testing_level()),
            rocket_project::RocketDesignStatus::Revising { remaining_indices, .. } =>
                format!("Revising {} flaw(s)", remaining_indices.len()),
        };

        let auto_target = company.auto_build_targets.get(&project.project_id).copied().unwrap_or(0);
        let auto_suffix = if !selected && auto_target > 0 {
            format!("  [A:{}]", auto_target)
        } else {
            String::new()
        };
        let line_text = format!(
            "  {} {} (Rev {})  {}{}",
            marker, project.design.name, project.revision, status_str, auto_suffix,
        );
        let text_width = line_text.len() as u16;

        // Track gauge data for this line
        let line_idx = lines.len();
        match &project.status {
            rocket_project::RocketDesignStatus::InDesign { work_completed, work_required } => {
                let ratio = work_completed / work_required;
                gauges.push(GaugeInfo {
                    line_index: line_idx, ratio,
                    label: format!("{:.0}/{:.0}", work_completed, work_required),
                    fill_color: Color::Rgb(0, 140, 140), text_width, right_aligned: false,
                });
            }
            rocket_project::RocketDesignStatus::Testing { work_completed } => {
                let ratio = work_completed / 30.0;
                gauges.push(GaugeInfo {
                    line_index: line_idx, ratio,
                    label: format!("{:.0}/30", work_completed),
                    fill_color: Color::Green, text_width, right_aligned: false,
                });
            }
            rocket_project::RocketDesignStatus::Revising { work_completed, .. } => {
                let ratio = work_completed / 30.0;
                gauges.push(GaugeInfo {
                    line_index: line_idx, ratio,
                    label: format!("{:.0}/30", work_completed),
                    fill_color: Color::Rgb(180, 130, 0), text_width, right_aligned: false,
                });
            }
        }

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

        lines.push(Line::from(Span::styled(line_text, style)));

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
                                "        ⚠ {}: {} ({})",
                                flaw.description, consequence_str, format_flaw_rate(flaw),
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

            // Auto-build target
            let auto_target = company.auto_build_targets.get(&project.project_id).copied().unwrap_or(0);
            if auto_target > 0 {
                lines.push(Line::from(format!("      Auto-build: {}", auto_target)));
            } else {
                lines.push(Line::from("      Auto-build: off"));
            }
        }
    }

    lines.push(Line::from(""));
    let mut controls = vec!["[N] New design"];
    if !company.rocket_projects.is_empty() {
        controls.extend_from_slice(&[
            "[+] Add team", "[-] Remove team",
            "[R] Revise", "[O] Order build", "[M] Auto-build", "[E] Hire eng team",
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
    render_gauges(frame, area, &gauges);
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
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    // Show floor space construction
    for order in &mfg.floor_space.under_construction {
        let line_text = format!("    Building {} unit(s)", order.units);
        let text_width = line_text.len() as u16;
        let line_idx = lines.len();
        let ratio = (crate::manufacturing::FLOOR_SPACE_BUILD_DAYS - order.days_remaining) as f64
            / crate::manufacturing::FLOOR_SPACE_BUILD_DAYS as f64;
        gauges.push(GaugeInfo {
            line_index: line_idx, ratio,
            label: format!("{}d left", order.days_remaining),
            fill_color: Color::Green, text_width, right_aligned: true,
        });
        lines.push(Line::from(line_text));
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
            format!("Waiting  Teams: {}", order.teams_assigned)
        } else {
            format!("Teams: {}", order.teams_assigned)
        };

        let line_text = format!(
            "    {} [{}] {} \"{}\"  {}",
            marker, i + 1, order.type_label(), order.display_name(), status_str,
        );
        let text_width = line_text.len() as u16;

        // Add gauge for active (non-waiting) orders
        if !order.waiting_for_prerequisites {
            let line_idx = lines.len();
            let fill_color = match &order.order_type {
                ManufacturingOrderType::Engine { .. } => Color::Cyan,
                ManufacturingOrderType::Stage { .. } => Color::Blue,
                ManufacturingOrderType::RocketIntegration { .. } => Color::Magenta,
            };
            gauges.push(GaugeInfo {
                line_index: line_idx,
                ratio: order.progress(),
                label: format!("{:.0}/{:.0}", order.work_completed, order.work_required),
                fill_color, text_width, right_aligned: true,
            });
        }

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(line_text, style)));
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
        "  [B] Buy floor space ($5M)  [+] Add mfg team  [-] Remove mfg team  [M] Hire mfg team",
        Style::default().fg(Color::Cyan),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Manufacturing ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
    render_gauges(frame, area, &gauges);
}

/// How ready the player is to fulfill a contract.
enum ContractReadiness {
    /// A built rocket in inventory can deliver the payload.
    Ready,
    /// A capable design exists but no rocket is built yet.
    NeedsBuild,
    /// No design (Testing or later) can deliver the required payload.
    Impossible,
}

fn check_contract_readiness(contract: &Contract, company: &Company) -> ContractReadiness {
    for project in &company.rocket_projects {
        // Only consider designs that are past InDesign (Testing, Revising, or Complete equivalent)
        if matches!(project.status, rocket_project::RocketDesignStatus::InDesign { .. }) {
            continue;
        }
        let max_payload = rocket_project::max_payload_to(
            &project.design, "earth_surface", &contract.destination,
        );
        if max_payload >= contract.payload_kg {
            if company.manufacturing.inventory.rocket_count(project.project_id) > 0 {
                return ContractReadiness::Ready;
            } else {
                return ContractReadiness::NeedsBuild;
            }
        }
    }
    ContractReadiness::Impossible
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
                match check_contract_readiness(c, &game.player_company) {
                    ContractReadiness::Ready => Style::default(),
                    ContractReadiness::NeedsBuild => Style::default().fg(Color::Yellow),
                    ContractReadiness::Impossible => Style::default().fg(Color::Red),
                }
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
                match check_contract_readiness(c, &game.player_company) {
                    ContractReadiness::Ready => Style::default().fg(Color::Green),
                    ContractReadiness::NeedsBuild => Style::default().fg(Color::Yellow),
                    ContractReadiness::Impossible => Style::default().fg(Color::Red),
                }
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

    // In-flight rockets
    lines.push(Line::from(Span::styled(
        "  ── In Flight ──",
        Style::default().fg(Color::DarkGray),
    )));

    let flights = &game.active_flights;
    if flights.is_empty() {
        lines.push(Line::from("  (no flights in transit)"));
    } else {
        for flight in flights.iter() {
            let dest_name = contract::destination_display_name(flight.destination());
            let eta = flight.eta_days();
            let eta_str = if eta == 1 { "1 day".to_string() } else { format!("{} days", eta) };
            let remaining_dv = flight.rocket.remaining_delta_v(&flight.design);
            lines.push(Line::from(vec![
                Span::styled("  ● ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{} → {}  ", flight.rocket_name, dest_name)),
                Span::styled(format!("ETA: {}  ", eta_str), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("Δv: {:.0} m/s", remaining_dv), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Spacecraft in orbit/on surface
    lines.push(Line::from(Span::styled(
        "  ── Spacecraft ──",
        Style::default().fg(Color::DarkGray),
    )));

    let spacecraft = &game.spacecraft;
    if spacecraft.is_empty() {
        lines.push(Line::from("  (no spacecraft)"));
    } else {
        for sc in spacecraft.iter() {
            let loc_name = contract::destination_display_name(&sc.location);
            let dv = sc.remaining_delta_v();
            let mut spans = vec![
                Span::styled("  ◆ ", Style::default().fg(Color::Green)),
                Span::raw(format!("{} @ {}  ", sc.name, loc_name)),
                Span::styled(format!("Δv: {:.0} m/s", dv), Style::default().fg(Color::DarkGray)),
            ];
            // Show current stage group if not on the final one
            let total_groups = sc.design.stage_groups.len();
            if total_groups > 1 {
                let current_group = (0..total_groups)
                    .find(|&gi| sc.rocket.stage_states.get(gi)
                        .map(|ss| ss.iter().any(|s| s.attached))
                        .unwrap_or(false));
                if let Some(gi) = current_group {
                    if gi + 1 < total_groups {
                        spans.push(Span::styled(
                            format!("  Stage {}/{}", gi + 1, total_groups),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
            }
            lines.push(Line::from(spans));
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
        .title(" Launches [L]aunch [U]ndisposable [F]ly [P]lan ");
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
                EventImportance::Critical => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
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
            EventImportance::Critical => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
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
                    if s.overexpansion_loss >= 0.5 {
                        loss_parts.push(format!("nozzle: -{:.0}", s.overexpansion_loss));
                    }
                    if !loss_parts.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("{}                 ({})", group_indent, loss_parts.join("  ")),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }

                // Overexpansion warning for first stage group launching from atmosphere
                if gi == 0 && state.launch_from == "earth_surface" {
                    let ambient = 101_325.0_f64;
                    let isp_frac = stage.engine.isp_fraction_at(ambient);
                    let risk = stage.engine.overexpansion_destruction_risk(ambient);
                    if risk > 0.0 {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "        ⚠ Flow separation risk: {:.0}%/engine, Isp penalty: {:.0}%  (exit {:.0} kPa)",
                                risk * 100.0,
                                (1.0 - isp_frac) * 100.0,
                                stage.engine.exit_pressure_pa / 1000.0,
                            ),
                            Style::default().fg(Color::Red),
                        )));
                    } else if isp_frac < 0.95 {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "        Isp penalty: {:.0}%  (exit {:.0} kPa at sea level)",
                                (1.0 - isp_frac) * 100.0,
                                stage.engine.exit_pressure_pa / 1000.0,
                            ),
                            Style::default().fg(Color::Yellow),
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
        InputMode::SelectScale { name, cycle, preset, scale, use_vacuum, vacuum_only } => {
            let baseline = engine_project::engine_baseline(*cycle, *preset);
            let mut lines = vec![
                Line::from(format!("  Design: {}  {:?}  {}", name, cycle, preset.name())),
                Line::from(""),
            ];
            if let Some(b) = baseline {
                let thrust = b.thrust_n * scale;
                let mass = b.mass_kg * scale;
                let isp = if *use_vacuum { b.isp_vac_s } else { b.isp_sl_s };
                let exit_p = if *use_vacuum { b.exit_pressure_vac_pa } else { b.exit_pressure_sl_pa };
                lines.push(Line::from(format!("  Scale: {:.2}x  [↑↓ to adjust]", scale)));
                lines.push(Line::from(""));
                lines.push(Line::from(format!("  Thrust: {:.0} kN", thrust / 1000.0)));
                lines.push(Line::from(format!("  Mass:   {:.0} kg", mass)));
                lines.push(Line::from(format!(
                    "  Isp:    {:.0} s ({})",
                    isp,
                    if *use_vacuum { "vacuum" } else { "sea level" },
                )));
                lines.push(Line::from(format!("  Exit P: {:.0} kPa", exit_p / 1000.0)));
                if *use_vacuum && !*vacuum_only {
                    lines.push(Line::from(Span::styled(
                        "  ⚠ Low exit pressure — risk of flow separation at sea level",
                        Style::default().fg(Color::Yellow),
                    )));
                }
                lines.push(Line::from(""));
                if *vacuum_only {
                    lines.push(Line::from(Span::styled(
                        "  Vacuum only (expander cycle)  [Enter] Confirm",
                        Style::default().fg(Color::Cyan),
                    )));
                } else {
                    lines.push(Line::from(Span::styled(
                        "  [V] Toggle vacuum/sea-level  [Enter] Confirm",
                        Style::default().fg(Color::Cyan),
                    )));
                }
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
        InputMode::LaunchSelectContract { rocket_item_id, selected, .. } => {
            let contracts = &app.game.player_company.active_contracts;
            let total_options = contracts.len() + 1;

            // Look up rocket design for ETA computation
            let rocket_design = app.game.player_company.manufacturing.inventory.rockets.iter()
                .find(|r| r.item_id == *rocket_item_id)
                .and_then(|r| {
                    app.game.player_company.rocket_projects.iter()
                        .find(|rp| rp.project_id == r.rocket_project_id)
                })
                .map(|rp| &rp.design);

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

                // Compute transit time estimate
                let transit_info = rocket_design.and_then(|design| {
                    let mass = design.total_mass_kg() + c.payload_kg;
                    let thrust = design.group_thrust_n(0);
                    let path = DELTA_V_MAP.shortest_path("earth_surface", &c.destination, mass)?;
                    let route = crate::flight::build_route(&path.0, mass, thrust);
                    let days: u32 = route.iter().map(|l| l.total_days()).sum();
                    Some((days, app.game.date.days_until(&c.deadline)))
                });

                let mut spans = vec![Span::styled(
                    format!("{}{} → {} ({:.0} kg, {})",
                        marker, c.name, dest_name, c.payload_kg, format_money(c.payment)),
                    style,
                )];

                if let Some((transit_days, days_to_deadline)) = transit_info {
                    let eta_str = format!("  ~{}d", transit_days);
                    if transit_days > days_to_deadline {
                        spans.push(Span::styled(eta_str, Style::default().fg(Color::Red)));
                        spans.push(Span::styled(" LATE!", Style::default().fg(Color::Red)));
                    } else {
                        spans.push(Span::styled(eta_str, Style::default().fg(Color::DarkGray)));
                    }
                }

                lines.push(Line::from(spans));
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
                let style = if i == state.selected_project && state.active_field == PlannerSetupField::Design {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
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
                let style = if i == state.selected_location && state.active_field == PlannerSetupField::Location {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {}{}", marker, display_name),
                    style,
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Tab] Switch field  [Enter] Start  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )));

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Δv Planner Setup ")
                .style(Style::default().fg(Color::Cyan));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::DvPlanner { state } => {
            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
            let loc_name = contract::destination_display_name(&state.current_location);
            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("  Location: "),
                    Span::styled(loc_name, Style::default().fg(Color::Cyan)),
                    Span::raw(format!("    Δv: {:.0} m/s", remaining_dv)),
                    Span::raw(format!("    Payload: {:.0} kg", state.payload_kg)),
                ]),
                Line::from(""),
            ];

            // Show planned route so far
            if !state.actions.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  Route:",
                    Style::default().fg(Color::DarkGray),
                )));
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
            lines.push(Line::from(Span::styled(
                "  Destinations:",
                Style::default().fg(Color::DarkGray),
            )));
            if state.destinations.is_empty() {
                lines.push(Line::from("  (no reachable destinations)"));
            } else {
                for (i, (_, display_name, dv_cost)) in state.destinations.iter().enumerate() {
                    let marker = if i == state.selected { " ▶ " } else { "   " };
                    let style = if i == state.selected {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    };
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
            lines.push(Line::from(Span::styled(
                "  [Enter] Go  [D] Drop payload  [U] Undo  [Esc] Close",
                Style::default().fg(Color::DarkGray),
            )));

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Δv Planner ")
                .style(Style::default().fg(Color::Cyan));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::FlySelectSpacecraft { selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Select spacecraft:"),
                Line::from(""),
            ];
            for (i, sc) in app.game.spacecraft.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                let dv = sc.remaining_delta_v();
                lines.push(Line::from(vec![
                    Span::styled(format!("{}{}", marker, sc.name), style),
                    Span::styled(
                        format!("  @ {}  Δv: {:.0} m/s", sc.location, dv),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] Select  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )));
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Fly Spacecraft ")
                .style(Style::default().fg(Color::Cyan));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::FlySelectDestination { destinations, remaining_dv, selected, .. } => {
            let mut lines = vec![
                Line::from(""),
                Line::from(format!("  Remaining Δv: {:.0} m/s", remaining_dv)),
                Line::from(""),
                Line::from("  Select destination:"),
                Line::from(""),
            ];
            for (i, (_, display_name, dv_cost)) in destinations.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
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
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Fly Spacecraft ")
                .style(Style::default().fg(Color::Cyan));
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

/// Minimum gauge width.
const MIN_GAUGE_WIDTH: u16 = 12;
/// Fixed gauge width for right-aligned gauges.
const RIGHT_GAUGE_WIDTH: u16 = 20;

/// A progress gauge to overlay on a line.
struct GaugeInfo {
    line_index: usize,
    ratio: f64,
    label: String,
    fill_color: Color,
    /// Column where the text on this line ends (for positioning gauge after text).
    text_width: u16,
    /// If true, use fixed-width right-aligned positioning instead of text-adjacent.
    right_aligned: bool,
}


/// Render Gauge overlays on top of a paragraph block.
/// `area` is the outer Rect of the containing block (with borders).
fn render_gauges(
    frame: &mut Frame,
    area: Rect,
    gauges: &[GaugeInfo],
) {
    // Content area inside the block borders
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    for gauge_info in gauges {
        let gauge_y = inner.y + gauge_info.line_index as u16;
        if gauge_y >= inner.bottom() {
            continue; // clipped
        }

        let (gauge_x, gauge_width) = if gauge_info.right_aligned {
            if inner.width < RIGHT_GAUGE_WIDTH + 2 {
                continue;
            }
            let x = inner.right().saturating_sub(RIGHT_GAUGE_WIDTH);
            (x, RIGHT_GAUGE_WIDTH)
        } else {
            // Position gauge after text with a 1-char gap
            let x = inner.x + gauge_info.text_width + 1;
            let w = inner.right().saturating_sub(x);
            if w < MIN_GAUGE_WIDTH {
                continue;
            }
            (x, w)
        };

        let gauge_area = Rect {
            x: gauge_x,
            y: gauge_y,
            width: gauge_width,
            height: 1,
        };
        let gauge = Gauge::default()
            .ratio(gauge_info.ratio.clamp(0.0, 1.0))
            .label(Span::styled(
                gauge_info.label.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ))
            .gauge_style(Style::default().fg(gauge_info.fill_color).bg(Color::DarkGray));
        frame.render_widget(gauge, gauge_area);
    }
}
