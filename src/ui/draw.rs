use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph};

use crate::contract::{self, Contract};
use crate::engine::EngineCycle;
use crate::engine_project::{EngineDesignStatus, EngineSource};
use crate::game_state::Company;
use crate::manufacturing::ManufacturingOrderType;
use crate::rocket_project;
use crate::event::EventImportance;
use crate::flaw::{Flaw, FlawConsequence, FlawTrigger};
use crate::launch::LaunchOutcome;
use crate::location::DELTA_V_MAP;
use crate::rocket;
use crate::ui::{App, FocusedPane, InputMode, RocketDesignerState, Tab};

/// Deduplicated list of destinations served by the player's currently-active
/// markets — including markets that haven't generated a contract this month.
/// Falls back to the basic Earth-orbit set (LEO, MEO, GTO, GEO) when no
/// markets are active yet.
fn relevant_destinations<'a>(game: &'a crate::game_state::GameState) -> Vec<&'a str> {
    let mut dests: Vec<&str> = Vec::new();
    for market in &game.markets {
        if !market.active {
            continue;
        }
        for d in &market.destinations {
            let id = d.location_id.as_str();
            if !dests.contains(&id) {
                dests.push(id);
            }
        }
    }
    if dests.is_empty() {
        dests.extend(["leo", "meo", "gto", "geo"]);
    }
    dests
}

fn format_dv(dv: f64) -> String {
    if dv.is_infinite() { "∞".to_string() }
    else { format!("{:.0} m/s", dv) }
}

/// Electrical power in human-readable units. Switches to kW above
/// 1 kW and MW above 1 MW so reactor outputs don't read as
/// "500000 W".
fn format_power_w(w: f64) -> String {
    if w.abs() >= 1_000_000.0 {
        format!("{:.2} MW", w / 1_000_000.0)
    } else if w.abs() >= 10_000.0 {
        format!("{:.0} kW", w / 1_000.0)
    } else if w.abs() >= 1_000.0 {
        format!("{:.1} kW", w / 1_000.0)
    } else {
        format!("{:.0} W", w)
    }
}

/// Mass in kilograms with thousands-separator commas. Reactor masses
/// hit five+ figures at scale 1.0; the unspaced number is hard to read.
fn format_kg(kg: f64) -> String {
    let int = kg.round() as i64;
    let sign = if int < 0 { "-" } else { "" };
    let mut digits = int.unsigned_abs().to_string();
    // Insert commas every 3 digits from the right.
    let mut i = digits.len();
    while i > 3 {
        i -= 3;
        digits.insert(i, ',');
    }
    format!("{}{} kg", sign, digits)
}

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
        Tab::Reactors => draw_reactors_tab(frame, app, area, border_style),
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
    let visible_engines: Vec<(usize, &crate::engine_project::EngineProject)> =
        company.visible_engine_projects().collect();

    let mut lines = vec![
        Line::from(format!("  Engine Projects ({})", visible_engines.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    if visible_engines.is_empty() {
        lines.push(Line::from("  No engine projects yet. Design one inside a rocket."));
    }

    for (i, (_orig_idx, project)) in visible_engines.iter().enumerate() {
        let selected = i == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = match &project.status {
            EngineDesignStatus::Proposed { .. } => unreachable!("filtered above"),
            EngineDesignStatus::InDesign { .. } => "In Design".to_string(),
            EngineDesignStatus::Testing { .. } =>
                format!("Testing  {}", project.testing_level()),
            EngineDesignStatus::Revising { remaining_flaw_indices, remaining_improvement_indices, .. } =>
                format!("Revising {} flaw(s), {} improvement(s)",
                    remaining_flaw_indices.len(), remaining_improvement_indices.len()),
        };

        let line_text = format!(
            "  {} {} (Rev {})  {}",
            marker, project.design.name, project.revision, status_str,
        );
        let text_width = line_text.len() as u16;

        // Track gauge data for this line
        let line_idx = lines.len();
        match &project.status {
            EngineDesignStatus::Proposed { .. } => unreachable!("filtered above"),
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
                EngineCycle::NuclearThermal => "Nuclear Thermal",
                EngineCycle::ElectricPropulsion => "Electric Propulsion",
                EngineCycle::SolarSail => "Solar Sail",
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
            let power_str = if project.design.power_draw_w > 0.0 {
                format!("    Power: {}",
                    format_power(project.design.power_draw_w))
            } else {
                String::new()
            };
            lines.push(Line::from(format!(
                "      Mass: {:.0} kg    Teams: {}    Scale: {:.2}x{}",
                project.design.mass_kg,
                project.teams_assigned,
                project.scale,
                power_str,
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

            // Show improvements
            let pending: Vec<_> = project.improvements.iter().filter(|i| !i.actualized).collect();
            let actualized: Vec<_> = project.improvements.iter().filter(|i| i.actualized).collect();
            if !pending.is_empty() || !actualized.is_empty() {
                for imp in &actualized {
                    lines.push(Line::from(Span::styled(
                        format!("        ✓ {}: {}", imp.description, imp.kind),
                        Style::default().fg(Color::Green),
                    )));
                }
                for imp in &pending {
                    lines.push(Line::from(Span::styled(
                        format!("        ★ {}: {} (pending revision)", imp.description, imp.kind),
                        Style::default().fg(Color::Cyan),
                    )));
                }
            }

            // Show tech deficiencies
            if !project.tech_deficiency_ids.is_empty() {
                if let Some(tech_id) = project.technology_id {
                    if let Some(tech) = app.game.technologies.iter().find(|t| t.id == tech_id) {
                        lines.push(Line::from(format!(
                            "      Tech deficiencies ({}):", tech.name,
                        )));
                        for def_id in &project.tech_deficiency_ids {
                            if let Some(def) = tech.deficiencies.iter().find(|d| d.id == *def_id) {
                                let status = if def.solved {
                                    "(solved elsewhere — easy fix)".to_string()
                                } else if def.total_attempts > 0 {
                                    format!("({} failed attempt{})",
                                        def.total_attempts,
                                        if def.total_attempts == 1 { "" } else { "s" })
                                } else {
                                    String::new()
                                };
                                lines.push(Line::from(Span::styled(
                                    format!("        ◆ {}: {} {}", def.description, def.kind, status),
                                    Style::default().fg(Color::Magenta),
                                )));
                            }
                        }
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

fn draw_reactors_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    use crate::reactor_project::{ReactorDesignStatus, ReactorProject};

    let company = &app.game.player_company;
    let visible: Vec<(usize, &ReactorProject)> = company.visible_reactor_projects().collect();

    let mut lines = vec![
        Line::from(format!("  Reactor Projects ({})", visible.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    if visible.is_empty() {
        lines.push(Line::from("  No reactor projects yet."));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press [N] to design one.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    for (i, (_real_idx, project)) in visible.iter().enumerate() {
        let selected = i == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = match &project.status {
            ReactorDesignStatus::Proposed { .. } => unreachable!("filtered above"),
            ReactorDesignStatus::InDesign { .. } => "In Design".to_string(),
            ReactorDesignStatus::Testing { .. } => "Testing".to_string(),
            ReactorDesignStatus::Revising { remaining_flaw_indices, .. } =>
                format!("Revising {} flaw(s)", remaining_flaw_indices.len()),
        };

        let line_text = format!(
            "  {} {} (Rev {})  {}",
            marker, project.design.name, project.revision, status_str,
        );
        let text_width = line_text.len() as u16;

        let line_idx = lines.len();
        if let ReactorDesignStatus::InDesign { work_completed, work_required } = &project.status {
            let ratio = work_completed / work_required;
            gauges.push(GaugeInfo {
                line_index: line_idx, ratio,
                label: format!("{:.0}/{:.0}", work_completed, work_required),
                fill_color: Color::Rgb(0, 140, 140), text_width, right_aligned: false,
            });
        }

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(line_text, style)));

        if selected {
            let d = &project.design;
            lines.push(Line::from(format!(
                "      {} • scale {:.2} • {} • {:.0} K",
                d.enrichment.display_name(),
                d.scale,
                format_power_w(d.steady_w),
                d.temperature_k,
            )));
            lines.push(Line::from(format!(
                "      mass {} (reactor {} + radiator {}) • ${:.1}M",
                format_kg(d.mass_kg),
                format_kg(d.reactor_mass_kg),
                format_kg(d.radiator.mass_kg),
                d.material_cost / 1_000_000.0,
            )));
            lines.push(Line::from(format!(
                "      Teams: {}  NRE: ${:.0}",
                project.teams_assigned, project.nre_cost,
            )));
        }
    }

    lines.push(Line::from(""));
    let controls: Vec<&str> = if visible.is_empty() {
        vec!["[N] New design"]
    } else {
        vec!["[N] New design", "[+] Add team", "[-] Remove team", "[E] Edit"]
    };
    lines.push(Line::from(Span::styled(
        format!("  {}", controls.join("  ")),
        Style::default().fg(Color::Cyan),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Reactors ");
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

            // Show engines used per stage group
            let mut seen_engines: Vec<(String, u32)> = Vec::new();
            for group in &project.design.stage_groups {
                for stage in group {
                    let rev = company.engine_projects.iter()
                        .find(|ep| ep.design.id == stage.engine.id)
                        .map(|ep| ep.revision)
                        .or_else(|| company.contracted_engines.iter()
                            .find(|ce| ce.design.id == stage.engine.id)
                            .map(|_| 0))
                        .unwrap_or(0);
                    let key = format!("{} Rev {}", stage.engine.name, rev);
                    if !seen_engines.iter().any(|(k, _)| k == &key) {
                        seen_engines.push((key, stage.engine_count));
                    } else if let Some(entry) = seen_engines.iter_mut().find(|(k, _)| k == &key) {
                        entry.1 += stage.engine_count;
                    }
                }
            }
            let engine_list: Vec<String> = seen_engines.iter()
                .map(|(name, count)| format!("{}x{}", count, name))
                .collect();
            lines.push(Line::from(format!(
                "      Engines: {}",
                engine_list.join(", "),
            )));

            // Initial acceleration at takeoff: stage 0, full propellant,
            // 0 payload, 1 AU.
            let avail_power = project.design.power_for_engines_w(1.0);
            let initial_thrust = project.design.group_effective_thrust_n(0, avail_power);
            let initial_mass = project.design.total_mass_kg();
            let initial_accel = if initial_mass > 0.0 {
                initial_thrust / initial_mass
            } else { 0.0 };
            lines.push(Line::from(format!(
                "      Total mass: {:.0} kg    dV: {:.0} m/s (0 payload)    Initial accel: {}",
                project.design.total_mass_kg(),
                project.design.total_delta_v(0.0),
                format_accel(initial_accel),
            )));

            // Show payload table for destinations served by active markets
            // (or the LEO/MEO/GTO/GEO fallback when none are active yet).
            let dests = relevant_destinations(&app.game);
            let table = crate::rocket_project::payload_table_for(
                &project.design, "earth_surface", &dests,
            );
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
            "[R] Revise", "[O] Order build", "[m] Auto-build",
            "[Shift+M] Modify", "[E] Hire eng team",
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
            // Group engines by name + revision
            let mut engine_counts: Vec<(&str, u32, usize)> = Vec::new();
            for eng in &mfg.inventory.engines {
                if let Some(entry) = engine_counts.iter_mut()
                    .find(|(n, r, _)| *n == eng.engine_name.as_str() && *r == eng.revision)
                {
                    entry.2 += 1;
                } else {
                    engine_counts.push((&eng.engine_name, eng.revision, 1));
                }
            }
            for (name, rev, count) in &engine_counts {
                lines.push(Line::from(format!("    {} Rev {}: {}", name, rev, count)));
            }
        }
        if !mfg.inventory.stages.is_empty() {
            lines.push(Line::from(format!("    Stages: {}", mfg.inventory.stages.len())));
        }
        if !mfg.inventory.rockets.is_empty() {
            for rocket_inv in &mfg.inventory.rockets {
                lines.push(Line::from(format!(
                    "    Rocket: {} Rev {}", rocket_inv.rocket_name, rocket_inv.revision
                )));
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

    // Available contracts grouped by market
    // Collect market IDs that have contracts
    let active_markets: Vec<&contract::Market> = game.markets.iter()
        .filter(|m| m.active && available.iter().any(|c| c.market_id == m.id))
        .collect();

    if available.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ── Available Contracts ──",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from("  (none available — wait for next month)"));
    } else {
        // Show contracts grouped by market
        for market in &active_markets {
            let market_contracts: Vec<(usize, &Contract)> = available.iter()
                .enumerate()
                .filter(|(_, c)| c.market_id == market.id)
                .collect();
            if market_contracts.is_empty() { continue; }

            // Market header with modifier info
            let mut header = format!("  ── {} ──", market.name);
            for modifier in &market.modifiers {
                header.push_str(&format!("  ({})", modifier.description));
            }
            lines.push(Line::from(Span::styled(
                header,
                Style::default().fg(Color::DarkGray),
            )));

            for (i, c) in market_contracts {
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

        // Show any contracts from unknown/legacy markets (no market_id match)
        let orphan_contracts: Vec<(usize, &Contract)> = available.iter()
            .enumerate()
            .filter(|(_, c)| !game.markets.iter().any(|m| m.id == c.market_id))
            .collect();
        if !orphan_contracts.is_empty() {
            lines.push(Line::from(Span::styled(
                "  ── Other ──",
                Style::default().fg(Color::DarkGray),
            )));
            for (i, c) in orphan_contracts {
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
                format!("{}{} (Rev {}){}", marker, r.rocket_name, r.revision, payload_info),
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
            let final_dest = contract::destination_display_name(flight.destination());
            let eta = flight.eta_days();
            let eta_str = if eta == 1 { "1 day".to_string() } else { format!("{} days", eta) };
            let remaining_dv = flight.rocket.remaining_delta_v(&flight.design);
            let current_loc = contract::destination_display_name(&flight.current_location);

            // First line: rocket name, phase, current → next leg, final destination
            let total_legs = flight.route.len();
            let current_leg_num = (flight.current_leg + 1).min(total_legs);
            let next_hop = flight.route.get(flight.current_leg)
                .map(|leg| contract::destination_display_name(&leg.to));
            let phase_prefix = flight.current_phase()
                .map(|p| format!("{}: ", p.word()))
                .unwrap_or_default();
            let progress_str = if let Some(next) = next_hop {
                if next != final_dest {
                    format!("{}{} → {} (leg {}/{}, final: {})",
                        phase_prefix, current_loc, next, current_leg_num, total_legs, final_dest)
                } else {
                    format!("{}{} → {} (leg {}/{})",
                        phase_prefix, current_loc, final_dest, current_leg_num, total_legs)
                }
            } else {
                format!("{}{} → {}", phase_prefix, current_loc, final_dest)
            };

            lines.push(Line::from(vec![
                Span::styled("  ● ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}  ", flight.rocket_name)),
                Span::styled(progress_str, Style::default().fg(Color::White)),
            ]));

            // Second line: leg progress, ETA, dv
            let leg_progress = flight.route.get(flight.current_leg).map(|leg| {
                let total = leg.total_days();
                let elapsed = total.saturating_sub(flight.leg_days_remaining);
                format!("leg: {}/{} days", elapsed, total)
            }).unwrap_or_default();

            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(format!("{}  ", leg_progress), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("ETA: {}  ", eta_str), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("Δv: {}", format_dv(remaining_dv)), Style::default().fg(Color::DarkGray)),
            ]));

            // Per-stage dv breakdown (for multi-stage rockets)
            if flight.design.stage_groups.len() > 1 {
                let mut stage_parts = Vec::new();
                for gi in 0..flight.design.stage_groups.len() {
                    let attached = flight.rocket.stage_states.get(gi)
                        .map_or(false, |ss| ss.iter().any(|s| s.attached));
                    if !attached {
                        continue;
                    }
                    let stage_dv = flight.rocket.group_remaining_delta_v(&flight.design, gi);
                    stage_parts.push(format!("S{}: {}", gi + 1, format_dv(stage_dv)));
                }
                if !stage_parts.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("      Stages: {}", stage_parts.join(", ")),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }

            // Current acceleration of the active stage group (with the
            // power derate applied at the flight's current sun distance).
            let active_group = (0..flight.design.stage_groups.len())
                .find(|&gi| flight.rocket.stage_states.get(gi)
                    .map(|ss| ss.iter().any(|s| s.attached))
                    .unwrap_or(false));
            if let Some(gi) = active_group {
                let stage_mass: f64 = flight.design.stage_groups.iter().enumerate()
                    .flat_map(|(gj, group)| {
                        let states = &flight.rocket.stage_states;
                        group.iter().enumerate().filter_map(move |(sj, stage)| {
                            let attached = states.get(gj).and_then(|g| g.get(sj))
                                .map_or(false, |s| s.attached);
                            if !attached { return None; }
                            let prop = states[gj][sj].propellant_remaining_kg;
                            Some(stage.dry_mass_kg() + prop)
                        })
                    })
                    .sum();
                let payload_mass: f64 = flight.payloads.iter().map(|p| p.mass_kg()).sum();
                let total_mass = stage_mass + payload_mass;
                let sun_au = DELTA_V_MAP.location(&flight.current_location)
                    .map_or(1.0, |l| l.sun_distance_au());
                let avail_power = flight.design.power_for_engines_w(sun_au);
                let thrust = flight.design.group_effective_thrust_n(gi, avail_power);
                let accel = if total_mass > 0.0 { thrust / total_mass } else { 0.0 };
                lines.push(Line::from(Span::styled(
                    format!("      Accel: {}", format_accel(accel)),
                    Style::default().fg(Color::DarkGray),
                )));
            }

            // Per-leg Δv plan: which stage(s) burn for each remaining leg.
            let plan = flight.dv_plan();
            if !plan.is_empty() {
                let mut leg_parts = Vec::new();
                for (offset, contributions) in plan.iter().enumerate() {
                    if contributions.is_empty() {
                        continue;
                    }
                    let leg_num = flight.current_leg + offset + 1;
                    let stage_strs: Vec<String> = contributions.iter()
                        .map(|(gi, dv)| format!("S{} {}", gi + 1, format_dv(*dv)))
                        .collect();
                    leg_parts.push(format!("L{}: {}", leg_num, stage_strs.join(" + ")));
                }
                if !leg_parts.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("      Plan: {}", leg_parts.join(" | ")),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
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
                Span::styled(format!("Δv: {}", format_dv(dv)), Style::default().fg(Color::DarkGray)),
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

            // Per-stage dv breakdown for multi-stage spacecraft
            if total_groups > 1 {
                let mut stage_parts = Vec::new();
                for gi in 0..total_groups {
                    let attached = sc.rocket.stage_states.get(gi)
                        .map_or(false, |ss| ss.iter().any(|s| s.attached));
                    if !attached {
                        continue;
                    }
                    let stage_dv = sc.rocket.group_remaining_delta_v(&sc.design, gi);
                    stage_parts.push(format!("S{}: {}", gi + 1, format_dv(stage_dv)));
                }
                if !stage_parts.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("      Stages: {}", stage_parts.join(", ")),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }

            // Current acceleration of the active stage group (with the
            // power derate applied at the spacecraft's current sun
            // distance — a Mars-bound ion craft will read lower than at
            // Earth).
            let active_group = (0..total_groups)
                .find(|&gi| sc.rocket.stage_states.get(gi)
                    .map(|ss| ss.iter().any(|s| s.attached))
                    .unwrap_or(false));
            if let Some(gi) = active_group {
                let stage_mass: f64 = sc.design.stage_groups.iter().enumerate()
                    .flat_map(|(gj, group)| {
                        let states = &sc.rocket.stage_states;
                        group.iter().enumerate().filter_map(move |(sj, stage)| {
                            let attached = states.get(gj).and_then(|g| g.get(sj))
                                .map_or(false, |s| s.attached);
                            if !attached { return None; }
                            let prop = states[gj][sj].propellant_remaining_kg;
                            Some(stage.dry_mass_kg() + prop)
                        })
                    })
                    .sum();
                let payload_mass: f64 = sc.payloads.iter().map(|p| p.mass_kg()).sum();
                let total_mass = stage_mass + payload_mass;
                let sun_au = DELTA_V_MAP.location(&sc.location)
                    .map_or(1.0, |l| l.sun_distance_au());
                let avail_power = sc.design.power_for_engines_w(sun_au);
                let thrust = sc.design.group_effective_thrust_n(gi, avail_power);
                let accel = if total_mass > 0.0 { thrust / total_mass } else { 0.0 };
                lines.push(Line::from(Span::styled(
                    format!("      Accel: {}", format_accel(accel)),
                    Style::default().fg(Color::DarkGray),
                )));
            }

            // Show payloads still aboard (e.g. CSM still carrying LEM).
            if !sc.payloads.is_empty() {
                let parts: Vec<String> = sc.payloads.iter().map(|p| match p {
                    crate::flight::Payload::Spacecraft { name, deploy_at, .. } => {
                        match deploy_at {
                            Some(loc) => format!(
                                "{} → {}", name, contract::destination_display_name(loc)),
                            None => format!("{} (docked)", name),
                        }
                    }
                    crate::flight::Payload::ContractDelivery { payload_kg, .. } =>
                        format!("contract ({:.0} kg)", payload_kg),
                    crate::flight::Payload::TestMass { mass_kg } =>
                        format!("test mass ({:.0} kg)", mass_kg),
                }).collect();
                lines.push(Line::from(Span::styled(
                    format!("      Carrying: {}", parts.join(", ")),
                    Style::default().fg(Color::DarkGray),
                )));
            }
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
        .title(" Launches [L]aunch [K]eep [F]ly [D]ock [U]ndock [P]lan ");
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
            // Rocket NRE is just the rocket project's own NRE — engine NRE is
            // shown in the Engine Costs section so it isn't double-counted.
            let nre = rp.nre_cost;

            let cost_history = company.rocket_cost_history.get(&design_id);
            let built = cost_history.map_or(0, |h| h.len());

            let (avg_str, marginal_str) = if built > 0 {
                let history = cost_history.unwrap();
                let total_build: f64 = history.iter().sum();
                let avg = (nre + total_build) / built as f64;
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
                name, format_money(nre), avg_str, marginal_str, built
            )));
        }
    }

    // Engine Costs section
    let any_engines = !company.engine_projects.is_empty()
        || !company.contracted_engines.is_empty();
    if any_engines {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ── Engine Costs ──",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from("  Engine            NRE          Avg Cost     Marginal     Built"));
        lines.push(Line::from("  ─────────────────────────────────────────────────────────────────"));

        for ep in &company.engine_projects {
            let history = company.engine_cost_history.get(&ep.project_id);
            let built = *company.engine_build_counts.get(&ep.project_id).unwrap_or(&0);

            let (avg_str, marginal_str) = if built > 0 {
                if let Some(h) = history {
                    let sum: f64 = h.iter().sum();
                    let avg = (ep.nre_cost + sum) / built as f64;
                    let marginal = *h.last().unwrap();
                    (format_money(avg), format_money(marginal))
                } else {
                    ("—".to_string(), "—".to_string())
                }
            } else {
                ("—".to_string(), "—".to_string())
            };

            let name = if ep.design.name.len() > 18 {
                format!("{}…", &ep.design.name[..17])
            } else {
                ep.design.name.clone()
            };

            lines.push(Line::from(format!(
                "  {:<18} {:>12} {:>12} {:>12} {:>5}",
                name, format_money(ep.nre_cost), avg_str, marginal_str, built
            )));
        }

        // Contracted engines: flat per-unit cost, no NRE/avg.
        for ce in &company.contracted_engines {
            let built = *company.contracted_engine_build_counts
                .get(&ce.id).unwrap_or(&0);
            let name = if ce.design.name.len() > 18 {
                format!("{}…", &ce.design.name[..17])
            } else {
                ce.design.name.clone()
            };
            let marginal_str = format_money(ce.purchase_cost_per_unit);
            lines.push(Line::from(format!(
                "  {:<18} {:>12} {:>12} {:>12} {:>5}",
                name, "—", "—", marginal_str, built
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

    draw_rocket_designer_content(frame, app, state, outer[0]);

    // Help bar for designer
    let help_text = if let Some(ref msg) = app.status_message {
        format!(" {} ", msg)
    } else {
        " [Enter] Edit  [←→] Engines  [+/-] Prop  [A] Add  [I] Ins  [B] Booster  [W] Power  [X] Rem  [P] Payload  [L] Site  [M] Mission  [D] Done  [Esc] Cancel ".to_string()
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

fn draw_rocket_designer_content(frame: &mut Frame, app: &App, state: &RocketDesignerState, area: Rect) {
    let mut lines = Vec::new();

    // Launch site display name
    let launch_display = DELTA_V_MAP.location(state.launch_from)
        .map_or(state.launch_from, |l| l.display_name);
    let destination_display = DELTA_V_MAP.location(state.destination)
        .map_or(state.destination, |l| l.display_name);

    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        "  Launch: {}    Payload: {:.0} kg",
        launch_display, state.payload_kg,
    )));

    // Build a temporary RocketDesign to compute stats
    let temp_design = rocket::RocketDesign {
        id: rocket::RocketDesignId(0),
        name: state.rocket_name.clone(),
        stage_groups: state.stage_groups.clone(),
    };

    // Mission line: required dv / available dv / margin / ETA. Required
    // dv and the route are derived from the stage-aware path planner so
    // engine choice (low-thrust, etc) affects the answer. Available dv
    // is Tsiolkovsky-total — both numbers bake in the same launch-leg
    // loss budget (the transfer graph stores fueled-cost dv).
    let mission_line = if state.stage_groups.is_empty() {
        Line::from(Span::styled(
            format!("  Mission: {} → {}    (add a stage to see feasibility)",
                launch_display, destination_display),
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        let plan = DELTA_V_MAP.plan_mission(
            state.launch_from, state.destination, &temp_design, state.payload_kg,
        );
        match plan {
            crate::path_planning::MissionPlan::NoGraphPath => Line::from(Span::styled(
                format!("  Mission: {} → {}    UNREACHABLE — no route in Δv map",
                    launch_display, destination_display),
                Style::default().fg(Color::Red),
            )),
            crate::path_planning::MissionPlan::DvShortfall { min_required_dv, available_dv } => Line::from(Span::styled(
                format!("  Mission: {} → {}    UNREACHABLE — Δv shortfall: need ≥ {:.0}, have {:.0} (short {:.0} m/s)",
                    launch_display, destination_display,
                    min_required_dv, available_dv, min_required_dv - available_dv),
                Style::default().fg(Color::Red),
            )),
            crate::path_planning::MissionPlan::ClassMismatch { .. } => Line::from(Span::styled(
                format!("  Mission: {} → {}    UNREACHABLE — no route exists for this engine type",
                    launch_display, destination_display),
                Style::default().fg(Color::Red),
            )),
            crate::path_planning::MissionPlan::Reachable { path, dv: required_dv } => {
                let available_dv = temp_design.total_delta_v(state.payload_kg);
                let margin = available_dv - required_dv;
                let eta_days: u32 = path.windows(2)
                    .filter_map(|w| DELTA_V_MAP.transfer(w[0], w[1]))
                    .map(|t| t.transit_days)
                    .sum();
                let color = if margin < 0.0 { Color::Red }
                    else if margin < 500.0 { Color::Yellow }
                    else { Color::Green };
                let eta_str = if eta_days == 0 { "<1 d".to_string() }
                    else { format!("{} d", eta_days) };
                Line::from(Span::styled(
                    format!(
                        "  Mission: {} → {}    Req Δv: {:.0}    Avail: {:.0}    Margin: {:+.0} m/s    ETA: {}",
                        launch_display, destination_display,
                        required_dv, available_dv, margin, eta_str,
                    ),
                    Style::default().fg(color),
                ))
            }
        }
    };
    lines.push(mission_line);
    lines.push(Line::from(""));

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
                Some(EngineSource::PlayerDesign(pid)) => {
                    // Annotate engines that are still in design so the
                    // player can tell their rocket will wait on them.
                    let ep = app.game.player_company.find_engine_project(*pid);
                    match ep.map(|ep| &ep.status) {
                        Some(crate::engine_project::EngineDesignStatus::Proposed { .. }) => "[prop]",
                        Some(crate::engine_project::EngineDesignStatus::InDesign { .. }) => "[id]",
                        Some(crate::engine_project::EngineDesignStatus::Revising { .. }) => "[rev]",
                        _ => "",
                    }
                }
                _ => "",
            };
            let engine_label = format!("{}{}", stage.engine.name, tag);

            // Compute burn time: propellant_mass / (mass_flow_rate * engine_count)
            let burn_str = if stage.engine.is_solar_sail() {
                "   ∞".to_string()
            } else {
                let burn_time_s = {
                    let mfr = stage.engine.mass_flow_rate() * stage.engine_count as f64;
                    if mfr > 0.0 { stage.propellant_mass_kg / mfr } else { 0.0 }
                };
                if burn_time_s > 86400.0 {
                    format!("{:>4.0}d", burn_time_s / 86400.0)
                } else {
                    format!("{:>4.0}s", burn_time_s)
                }
            };

            // Stats only shown on the last inner stage of a group
            let is_last_in_group = si + 1 == group_len;
            let stat_str = if is_last_in_group {
                if let Some(s) = stats.get(gi) {
                    let eff_str = if s.delta_v_effective.is_infinite() { "     ∞".to_string() }
                        else { format!("{:>6.0}", s.delta_v_effective) };
                    let vac_str = if s.delta_v_vacuum.is_infinite() { "       ∞".to_string() }
                        else { format!("{:>8.0}", s.delta_v_vacuum) };
                    format!(
                        "{:>5}  {:>5.1}  {}  {}  {:>5.2}",
                        burn_str,
                        s.mass_ratio,
                        eff_str,
                        vac_str,
                        s.twr,
                    )
                } else {
                    format!("{:>5}", burn_str)
                }
            } else {
                format!("{:>5}", burn_str)
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

            // Per-stage power summary (compact)
            if !stage.power_sources.is_empty() {
                let supply: f64 = stage.power_sources.iter()
                    .map(|p| crate::rocket::stage_source_supply_w(stage, p, 1.0)).sum();
                let battery: f64 = stage.power_sources.iter()
                    .filter_map(|p| match p.kind {
                        crate::power::PowerSourceKind::Battery => Some(p.capacity_kwd),
                        _ => None,
                    }).sum();
                let demand = stage.housekeeping_w();
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}      power: {} src, {:.0}/{:.0} W @ 1AU, {:.2} kWd",
                        group_indent,
                        stage.power_sources.len(),
                        supply, demand, battery,
                    ),
                    Style::default().fg(Color::DarkGray),
                )));
            }

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
            "  Total dV: {} (vacuum: {})",
            format_dv(total_dv_effective), format_dv(total_dv_vacuum),
        )));
        lines.push(Line::from(format!(
            "  Total mass: {}",
            format_mass(total_mass),
        )));
        // Initial acceleration: stage 0 firing at 1 AU with all stages
        // attached and full propellant. Captures the power derate so
        // ion designs read low.
        let avail_power = temp_design.power_for_engines_w(1.0);
        let initial_thrust = temp_design.group_effective_thrust_n(0, avail_power);
        let initial_accel = if total_mass > 0.0 { initial_thrust / total_mass } else { 0.0 };
        lines.push(Line::from(format!(
            "  Initial accel: {}",
            format_accel(initial_accel),
        )));

        // Electrical summary. Read-only for now; editing UI is a follow-up.
        // Compute supply at takeoff (1 AU) and housekeeping demand across
        // attached stages; show whether designs balance.
        let mut total_housekeeping = 0.0;
        let mut total_supply_1au = 0.0;
        let mut total_battery_kwd = 0.0;
        let mut any_explicit = false;
        for group in &temp_design.stage_groups {
            for stage in group {
                total_housekeeping += stage.housekeeping_w();
                for src in &stage.power_sources {
                    any_explicit = true;
                    total_supply_1au += crate::rocket::stage_source_supply_w(stage, src, 1.0);
                    if let crate::power::PowerSourceKind::Battery = src.kind {
                        total_battery_kwd += src.capacity_kwd;
                    }
                }
            }
        }
        let summary = if any_explicit {
            let surplus = total_supply_1au - total_housekeeping;
            let surplus_marker = if surplus >= 0.0 { "+" } else { "" };
            format!(
                "  Power: {:.0} W supply (@ 1 AU)  /  {:.0} W demand  ({}{:.0} W)  battery: {:.2} kWd",
                total_supply_1au, total_housekeeping, surplus_marker, surplus, total_battery_kwd,
            )
        } else {
            format!(
                "  Power: no explicit sources (grandfathered)  housekeeping demand: {:.0} W",
                total_housekeeping,
            )
        };
        lines.push(Line::from(Span::styled(
            summary,
            if any_explicit && total_supply_1au < total_housekeeping {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )));
        lines.push(Line::from(""));

        // Payload feasibility for destinations served by active markets
        // (or the LEO/MEO/GTO/GEO fallback when none are active yet).
        let dests = relevant_destinations(&app.game);
        let table = rocket_project::payload_table_for(
            &temp_design, state.launch_from, &dests,
        );
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

    let title = if state.is_modify() {
        format!(" Rocket Designer — Modify \"{}\" ", state.rocket_name)
    } else {
        format!(" Rocket Designer: \"{}\" ", state.rocket_name)
    };
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
        InputMode::EngineEditor { project_id, cursor, .. } => {
            draw_engine_editor_modal(frame, app, *project_id, *cursor, None, modal_area);
        }
        InputMode::EngineEditorNameInput { project_id, cursor, buffer, .. } => {
            draw_engine_editor_modal(frame, app, *project_id, *cursor, Some(("Name", buffer.clone())), modal_area);
        }
        InputMode::EngineEditorScaleInput { project_id, cursor, buffer, .. } => {
            draw_engine_editor_modal(frame, app, *project_id, *cursor, Some(("Scale", buffer.clone())), modal_area);
        }
        InputMode::ReactorEditor { project_id, cursor } => {
            draw_reactor_editor_modal(frame, app, *project_id, *cursor, None, modal_area);
        }
        InputMode::ReactorEditorNameInput { project_id, cursor, buffer } => {
            draw_reactor_editor_modal(frame, app, *project_id, *cursor, Some(("Name", buffer.clone())), modal_area);
        }
        InputMode::ReactorEditorScaleInput { project_id, cursor, buffer } => {
            draw_reactor_editor_modal(frame, app, *project_id, *cursor, Some(("Scale", buffer.clone())), modal_area);
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
        InputMode::PowerEditor { state, group_index, stage_index, cursor } => {
            draw_power_editor_modal(
                frame, app, state, *group_index, *stage_index, *cursor, modal_area,
            );
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
                    if destination_conflict { "  ⚠ contracts disagree" } else { "" },
                )),
                Line::from(format!("  Payload mass: {}", format_mass(payload_mass))),
                Line::from(""),
            ];

            let mut row = 0usize;

            if !contracts.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  ── Contracts ──",
                    Style::default().fg(Color::DarkGray),
                )));
                for (i, c) in contracts.iter().enumerate() {
                    let mark = if *cursor == row { " ▶ " } else { "   " };
                    let check = if contract_picks[i] { "[✓]" } else { "[ ]" };
                    let dest_name = contract::destination_display_name(&c.destination);
                    let style = if *cursor == row {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{}{} {} → {} ({:.0} kg, {})",
                            mark, check, c.name, dest_name, c.payload_kg, format_money(c.payment)),
                        style,
                    )));
                    row += 1;
                }
                lines.push(Line::from(""));
            }

            if !spacecraft_item_ids.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  ── Spacecraft Payloads ──",
                    Style::default().fg(Color::DarkGray),
                )));
                for (i, item_id) in spacecraft_item_ids.iter().enumerate() {
                    let mark = if *cursor == row { " ▶ " } else { "   " };
                    let check = if spacecraft_picks[i] { "[✓]" } else { "[ ]" };
                    let style = if *cursor == row {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    };
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
            lines.push(Line::from(Span::styled(
                "  [Space] toggle  [Enter] launch  [Esc] cancel",
                Style::default().fg(Color::DarkGray),
            )));

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Launch Manifest ")
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
                    Span::raw(format!("    Δv: {}", format_dv(remaining_dv))),
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
                        format!("  @ {}  Δv: {}", sc.location, format_dv(dv)),
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
                Line::from(format!("  Remaining Δv: {}", format_dv(*remaining_dv))),
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
        InputMode::DockSelectSmall { selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Pick spacecraft to dock onto another:"),
                Line::from(""),
            ];
            for (i, sc) in app.game.spacecraft.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow)
                } else { Style::default() };
                let loc = contract::destination_display_name(&sc.location);
                lines.push(Line::from(Span::styled(
                    format!("{}{}  @ {}", marker, sc.name, loc),
                    style,
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] Select  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )));
            let block = Block::default().borders(Borders::ALL)
                .title(" Dock — Pick Spacecraft ")
                .style(Style::default().fg(Color::Cyan));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::DockSelectLarge { small_idx, candidates, selected } => {
            let small_name = &app.game.spacecraft[*small_idx].name;
            let small_loc = contract::destination_display_name(
                &app.game.spacecraft[*small_idx].location);
            let mut lines = vec![
                Line::from(""),
                Line::from(format!("  Dock {} onto … (at {})", small_name, small_loc)),
                Line::from(""),
            ];
            for (i, &cand) in candidates.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow)
                } else { Style::default() };
                let sc = &app.game.spacecraft[cand];
                lines.push(Line::from(Span::styled(
                    format!("{}{}", marker, sc.name),
                    style,
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] Confirm  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )));
            let block = Block::default().borders(Borders::ALL)
                .title(" Dock — Pick Carrier ")
                .style(Style::default().fg(Color::Cyan));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::UndockSelectCarrier { candidates, selected } => {
            let mut lines = vec![
                Line::from(""),
                Line::from("  Pick carrier to undock from:"),
                Line::from(""),
            ];
            for (i, &cand) in candidates.iter().enumerate() {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow)
                } else { Style::default() };
                let sc = &app.game.spacecraft[cand];
                let loc = contract::destination_display_name(&sc.location);
                lines.push(Line::from(Span::styled(
                    format!("{}{}  @ {}", marker, sc.name, loc),
                    style,
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] Select  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )));
            let block = Block::default().borders(Borders::ALL)
                .title(" Undock — Pick Carrier ")
                .style(Style::default().fg(Color::Cyan));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
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
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow)
                } else { Style::default() };
                if let crate::flight::Payload::Spacecraft { name, .. } = &carrier.payloads[pi] {
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", marker, name),
                        style,
                    )));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] Confirm  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )));
            let block = Block::default().borders(Borders::ALL)
                .title(" Undock — Pick Payload ")
                .style(Style::default().fg(Color::Cyan));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
        InputMode::RocketDesignerLocationPicker { target, locations, selected, .. } => {
            let title = match target {
                crate::ui::LocationPickerTarget::LaunchSite => " Pick Launch Site ",
                crate::ui::LocationPickerTarget::MissionDestination => " Pick Mission Destination ",
            };
            let mut lines = vec![Line::from("")];
            // Visible window around the selected entry so long lists scroll.
            let modal_inner_h = modal_area.height.saturating_sub(4) as usize;
            let window = modal_inner_h.max(5);
            let start = selected.saturating_sub(window / 2).min(locations.len().saturating_sub(window).max(0));
            for (i, (_id, name)) in locations.iter().enumerate().skip(start).take(window) {
                let marker = if i == *selected { " ▶ " } else { "   " };
                let style = if i == *selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else { Style::default() };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", marker, name),
                    style,
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [↑↓] Move  [Enter] Confirm  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )));
            let block = Block::default().borders(Borders::ALL)
                .title(title)
                .style(Style::default().fg(Color::Yellow));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, modal_area);
        }
    }
}

/// Non-linear engine editor. The cursor walks five rows:
/// 0=Name, 1=Cycle, 2=Preset, 3=Scale, 4=Vacuum. When `text_input` is
/// Some, a sub-modal text/number entry is overlaid (for Name or Scale).
fn draw_engine_editor_modal(
    frame: &mut Frame,
    app: &App,
    project_id: crate::engine_project::EngineProjectId,
    cursor: usize,
    text_input: Option<(&str, String)>,
    area: Rect,
) {
    let ep = match app.game.player_company.find_engine_project(project_id) {
        Some(ep) => ep,
        None => return,
    };
    let baseline = crate::engine_project::engine_baseline(ep.design.cycle, ep.preset);
    let vacuum_only = baseline.map_or(false, |b| b.vacuum_only);
    let use_vacuum = !ep.design.needs_atmosphere;
    let row_count = if vacuum_only { 4 } else { 5 };
    let cursor = cursor.min(row_count - 1);

    let row_label = |row: usize, sel: bool| -> &'static str {
        if sel && cursor == row { "▶" } else { " " }
    };
    let row_style = |row: usize| -> Style {
        if cursor == row {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else { Style::default() }
    };

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" Status: {}", match &ep.status {
                crate::engine_project::EngineDesignStatus::Proposed { .. } => "Proposed",
                crate::engine_project::EngineDesignStatus::InDesign { .. } => "In Design",
                crate::engine_project::EngineDesignStatus::Testing { .. } => "Testing (read-only)",
                crate::engine_project::EngineDesignStatus::Revising { .. } => "Revising",
            }),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    lines.push(Line::from(Span::styled(
        format!(" {} Name:   {}", row_label(0, true), ep.design.name),
        row_style(0),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} Cycle:  {:?}", row_label(1, true), ep.design.cycle),
        row_style(1),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} Preset: {}", row_label(2, true), ep.preset.name()),
        row_style(2),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} Scale:  {:.3}×", row_label(3, true), ep.scale),
        row_style(3),
    )));
    if !vacuum_only {
        lines.push(Line::from(Span::styled(
            format!(" {} Vacuum: {}",
                row_label(4, true),
                if use_vacuum { "yes" } else { "no" }),
            row_style(4),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "   Vacuum: yes  (fixed)".to_string(),
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Live + baseline derived stats.
    lines.push(Line::from(""));
    if let Some(b) = baseline {
        lines.push(Line::from(Span::styled(
            format!(" Baseline ({:?} / {}):  thrust {:.0} kN  mass {:.0} kg  Isp {:.0} s",
                ep.design.cycle, ep.preset.name(),
                b.thrust_n / 1000.0, b.mass_kg,
                if use_vacuum { b.isp_vac_s } else { b.isp_sl_s }),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(format!(
        " Scaled:    thrust {:.0} kN  mass {:.0} kg  Isp {:.0} s  power {:.0} W",
        ep.design.thrust_n / 1000.0, ep.design.mass_kg, ep.design.isp_s, ep.design.power_draw_w,
    )));
    let (work_completed, work_required) = match &ep.status {
        crate::engine_project::EngineDesignStatus::Proposed { work_required } => (0.0, *work_required),
        crate::engine_project::EngineDesignStatus::InDesign { work_completed, work_required } => (*work_completed, *work_required),
        crate::engine_project::EngineDesignStatus::Revising { work_completed, .. } => (*work_completed, 0.0),
        crate::engine_project::EngineDesignStatus::Testing { work_completed } => (*work_completed, 0.0),
    };
    lines.push(Line::from(format!(
        " Complexity: {}    Work: {:.0} / {:.0}",
        ep.complexity, work_completed, work_required,
    )));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " [↑↓] Move  [←→] Change  [Enter] Edit text  [Esc] Done",
        Style::default().fg(Color::DarkGray),
    )));

    if let Some((field, buffer)) = text_input {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" Edit {}:  > {}█", field, buffer),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(Line::from(Span::styled(
            " [Enter] Apply   [Esc] Cancel".to_string(),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Engine Editor ")
        .style(Style::default().fg(Color::Yellow));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_reactor_editor_modal(
    frame: &mut Frame,
    app: &App,
    project_id: crate::reactor_project::ReactorProjectId,
    cursor: usize,
    text_input: Option<(&str, String)>,
    area: Rect,
) {
    use crate::reactor_project::ReactorDesignStatus;

    let rp = match app.game.player_company.find_reactor_project(project_id) {
        Some(rp) => rp,
        None => return,
    };
    const ROW_COUNT: usize = 2; // Name, Scale (Phase 2b adds Enrichment)
    let cursor = cursor.min(ROW_COUNT - 1);

    let row_label = |row: usize| -> &'static str {
        if cursor == row { "▶" } else { " " }
    };
    let row_style = |row: usize| -> Style {
        if cursor == row {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else { Style::default() }
    };

    let status_label = match &rp.status {
        ReactorDesignStatus::Proposed { .. } => "Proposed (new draft)",
        ReactorDesignStatus::InDesign { .. } => "In Design",
        ReactorDesignStatus::Testing { .. } => "Testing (read-only)",
        ReactorDesignStatus::Revising { .. } => "Revising",
    };

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" Status: {}", status_label),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {} Name:  {}", row_label(0), rp.design.name),
            row_style(0),
        )),
        Line::from(Span::styled(
            format!(" {} Scale: {:.3}×", row_label(1), rp.design.scale),
            row_style(1),
        )),
        Line::from(Span::styled(
            format!("   Enrichment: {}  (Phase 2b unlocks MEU/HEU)",
                rp.design.enrichment.display_name()),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(format!(
            " Output:  {}   Temperature: {:.0} K",
            format_power_w(rp.design.steady_w), rp.design.temperature_k,
        )),
        Line::from(format!(
            " Mass:    {} (reactor {} + radiator {})",
            format_kg(rp.design.mass_kg),
            format_kg(rp.design.reactor_mass_kg),
            format_kg(rp.design.radiator.mass_kg),
        )),
        Line::from(format!(
            " Material: ${:.1}M",
            rp.design.material_cost / 1_000_000.0,
        )),
    ];

    let (work_completed, work_required) = match &rp.status {
        ReactorDesignStatus::Proposed { work_required } => (0.0, *work_required),
        ReactorDesignStatus::InDesign { work_completed, work_required } =>
            (*work_completed, *work_required),
        ReactorDesignStatus::Testing { work_completed } => (*work_completed, 0.0),
        ReactorDesignStatus::Revising { work_completed, .. } => (*work_completed, 0.0),
    };
    lines.push(Line::from(format!(
        " Complexity: {}    Work: {:.0} / {:.0}",
        rp.complexity, work_completed, work_required,
    )));

    lines.push(Line::from(""));
    let footer = if matches!(rp.status, ReactorDesignStatus::Proposed { .. }) {
        " [↑↓] Field  [←→] Scale  [Enter] Edit text  [D] Done  [Esc] Cancel"
    } else {
        " [↑↓] Field  [←→] Scale  [Enter] Edit text  [Esc] Close"
    };
    lines.push(Line::from(Span::styled(
        footer.to_string(),
        Style::default().fg(Color::DarkGray),
    )));

    if let Some((field, buffer)) = text_input {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" Edit {}:  > {}█", field, buffer),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(Line::from(Span::styled(
            " [Enter] Apply   [Esc] Cancel".to_string(),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Reactor Editor ")
        .style(Style::default().fg(Color::Yellow));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
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

    // Helper: tag an engine source with its current status so the
    // player can tell in-design from testing engines at a glance.
    let status_tag = |source: &EngineSource| -> &'static str {
        match source {
            EngineSource::Contracted(_) => " [3P]",
            EngineSource::PlayerDesign(pid) => {
                app.game.player_company.find_engine_project(*pid)
                    .map(|ep| match ep.status {
                        crate::engine_project::EngineDesignStatus::Proposed { .. } => " [proposed]",
                        crate::engine_project::EngineDesignStatus::InDesign { .. } => " [in design]",
                        crate::engine_project::EngineDesignStatus::Revising { .. } => " [revising]",
                        crate::engine_project::EngineDesignStatus::Testing { .. } => "",
                    })
                    .unwrap_or("")
            }
        }
    };

    for (i, (source, design)) in engines.iter().enumerate() {
        let marker = if i == selected { "▶" } else { " " };
        let tag = status_tag(source);
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

    // "Design new engine" sentinel row — picking it opens the standard
    // engine-design wizard and returns to the rocket designer after.
    let new_engine_idx = engines.len();
    let marker = if selected == new_engine_idx { "▶" } else { " " };
    let style = if selected == new_engine_idx {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    lines.push(Line::from(Span::styled(
        format!("  {} + Design new engine…", marker),
        style,
    )));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Enter] Select  [E] Edit  [Esc] Back",
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

/// Format a power draw in watts, picking W / kW / MW for readability.
fn format_power(watts: f64) -> String {
    if watts >= 1_000_000.0 {
        format!("{:.1} MW", watts / 1_000_000.0)
    } else if watts >= 1_000.0 {
        format!("{:.1} kW", watts / 1_000.0)
    } else {
        format!("{:.0} W", watts)
    }
}

/// Format an acceleration in m/s² as a multiple of standard gravity,
/// scaling down to mg / μg / ng for low-thrust craft.
fn format_accel(a_m_s2: f64) -> String {
    let g = a_m_s2 / 9.80665;
    if g >= 1.0 {
        format!("{:.2} g", g)
    } else if g >= 1e-3 {
        format!("{:.0} mg", g * 1e3)
    } else if g >= 1e-6 {
        format!("{:.0} μg", g * 1e6)
    } else if g > 0.0 {
        format!("{:.0} ng", g * 1e9)
    } else {
        "0".to_string()
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

fn draw_power_editor_modal(
    frame: &mut Frame,
    app: &App,
    state: &RocketDesignerState,
    group_index: usize,
    stage_index: usize,
    cursor: usize,
    area: Rect,
) {
    use crate::power::{power_presets, preset_available, source_summary};

    let stage = match state.stage_groups
        .get(group_index)
        .and_then(|g| g.get(stage_index))
    {
        Some(s) => s,
        None => return,
    };
    let group_len = state.stage_groups[group_index].len();
    let stage_label = RocketDesignerState::stage_name(group_index, stage_index, group_len);

    // Live totals for the header. Two demands:
    //   - idle = housekeeping only (always-on draw).
    //   - thrust = housekeeping + engines at full power_draw_w * count.
    // For a chemical engine the two are equal; for an ion stage the
    // thrust draw can be orders of magnitude higher than housekeeping.
    let supply_w: f64 = stage.power_sources.iter()
        .map(|p| crate::rocket::stage_source_supply_w(stage, p, 1.0)).sum();
    let idle_demand_w = stage.housekeeping_w();
    let engine_draw_w = stage.engine.power_draw_w * stage.engine_count as f64;
    let thrust_demand_w = idle_demand_w + engine_draw_w;
    let battery_kwd: f64 = stage.power_sources.iter()
        .filter_map(|p| match p.kind {
            crate::power::PowerSourceKind::Battery => Some(p.capacity_kwd),
            _ => None,
        }).sum();

    let supply_color = if supply_w < idle_demand_w {
        Color::Red
    } else if supply_w < thrust_demand_w {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(format!("  Power editor — stage {}", stage_label)),
        Line::from(Span::styled(
            format!(
                "  Supply @ 1 AU: {:.0} W    Idle demand: {:.0} W    Thrust demand: {:.0} W    Battery: {:.2} kWd",
                supply_w, idle_demand_w, thrust_demand_w, battery_kwd,
            ),
            Style::default().fg(supply_color),
        )),
        Line::from(""),
    ];

    let n_equipped = stage.power_sources.len();
    let player_reactors: Vec<&crate::reactor_project::ReactorProject> =
        app.game.player_company.installable_reactor_projects().collect();
    // Filter the preset catalog to only those whose tech is unlocked.
    let presets: Vec<&crate::power::PowerPreset> = power_presets().iter()
        .filter(|p| preset_available(p, &app.game.technologies))
        .collect();
    let mut row = 0usize;

    // Equipped sources
    lines.push(Line::from(Span::styled(
        "  ── Equipped ──",
        Style::default().fg(Color::DarkGray),
    )));
    if n_equipped == 0 {
        lines.push(Line::from(Span::styled(
            "    (none)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    for src in &stage.power_sources {
        let mark = if cursor == row { " ▶ " } else { "   " };
        let style = if cursor == row {
            Style::default().fg(Color::Yellow)
        } else { Style::default() };
        lines.push(Line::from(Span::styled(
            format!("{}{}", mark, source_summary(src)),
            style,
        )));
        row += 1;
    }
    lines.push(Line::from(""));

    // Player-researched reactors (installable when Testing+).
    if !player_reactors.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ── Player Reactors ──",
            Style::default().fg(Color::DarkGray),
        )));
        for rp in &player_reactors {
            let mark = if cursor == row { " ▶ " } else { "   " };
            let style = if cursor == row {
                Style::default().fg(Color::Yellow)
            } else { Style::default() };
            lines.push(Line::from(Span::styled(
                format!(
                    "{}{}  ({}, {})",
                    mark, rp.design.name,
                    format_power_w(rp.design.steady_w),
                    format_kg(rp.design.mass_kg),
                ),
                style,
            )));
            row += 1;
        }
        lines.push(Line::from(""));
    }

    // Add presets
    lines.push(Line::from(Span::styled(
        "  ── Add ──",
        Style::default().fg(Color::DarkGray),
    )));
    for preset in presets {
        let mark = if cursor == row { " ▶ " } else { "   " };
        let style = if cursor == row {
            Style::default().fg(Color::Yellow)
        } else { Style::default() };
        lines.push(Line::from(Span::styled(
            format!("{}{}", mark, preset.label),
            style,
        )));
        row += 1;
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [↑↓] Navigate  [Space] Add  [+/-] Resize Panel  [X/Del] Remove  [Esc] Done",
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Power Editor — {} ", stage_label))
        .style(Style::default().fg(Color::Cyan));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod format_helpers_tests {
    use super::*;

    #[test]
    fn power_w_below_1k() {
        assert_eq!(format_power_w(500.0), "500 W");
        assert_eq!(format_power_w(999.0), "999 W");
    }

    #[test]
    fn power_kw_single_digit_keeps_decimal() {
        assert_eq!(format_power_w(1_000.0), "1.0 kW");
        assert_eq!(format_power_w(5_000.0), "5.0 kW");
        assert_eq!(format_power_w(9_999.0), "10.0 kW");
    }

    #[test]
    fn power_kw_double_digit_drops_decimal() {
        assert_eq!(format_power_w(10_000.0), "10 kW");
        assert_eq!(format_power_w(50_000.0), "50 kW");
        assert_eq!(format_power_w(500_000.0), "500 kW");
    }

    #[test]
    fn power_mw_uses_two_decimals() {
        assert_eq!(format_power_w(1_000_000.0), "1.00 MW");
        assert_eq!(format_power_w(5_500_000.0), "5.50 MW");
    }

    #[test]
    fn kg_with_commas() {
        assert_eq!(format_kg(0.0), "0 kg");
        assert_eq!(format_kg(800.0), "800 kg");
        assert_eq!(format_kg(4_200.0), "4,200 kg");
        assert_eq!(format_kg(33_348.0), "33,348 kg");
        assert_eq!(format_kg(1_234_567.0), "1,234,567 kg");
    }
}
