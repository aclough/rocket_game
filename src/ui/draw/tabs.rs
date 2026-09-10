//! The main tabs: the three project panes, manufacturing, launches,
//! finance and events.

use super::*;

// ── Project panes ────────────────────────────────────────────────────
//
// The Engines, Reactors and Rockets tabs are one list layout with a
// kind-specific detail block under the selected row. Everything that
// isn't the detail block is shared below.

/// One row of a project pane: marker, name, revision, status, plus a
/// progress gauge for the phase the project is in. `extra` goes after
/// the status (rockets: build time and the auto-build tag).
pub(super) fn push_project_row<D: Designable>(
    lines: &mut Vec<Line<'static>>,
    gauges: &mut Vec<GaugeInfo>,
    project: &DesignProject<D>,
    selected: bool,
    balance: &crate::balance_config::BalanceConfig,
    extra: &str,
) {
    let marker = if selected { "▶" } else { " " };
    let status_str = match &project.status {
        DesignStatus::Proposed { .. } => unreachable!("panes hide Proposed drafts"),
        DesignStatus::InDesign { .. } => "In Design".to_string(),
        DesignStatus::Testing { .. } => format!("Testing  {}", project.testing_level(balance)),
        DesignStatus::Revising { .. } => project.revision_plan()
            .map(|plan| plan.describe())
            .unwrap_or_default(),
    };
    let line_text = format!(
        "  {} {} (Rev {})  {}{}",
        marker, project.design.name(), project.revision, status_str, extra,
    );
    let text_width = line_text.chars().count() as u16;

    // Teal for design work, green for the current testing cycle, amber
    // for revision work.
    let (done, total, fill_color) = match &project.status {
        DesignStatus::Proposed { .. } => unreachable!("panes hide Proposed drafts"),
        DesignStatus::InDesign { work_completed, work_required } =>
            (*work_completed, *work_required, Color::Rgb(0, 140, 140)),
        DesignStatus::Testing { work_completed } =>
            (*work_completed, balance.work.testing_cycle_work, Color::Green),
        DesignStatus::Revising { work_completed, .. } =>
            (*work_completed, balance.work.flaw_revision_work, Color::Rgb(180, 130, 0)),
    };
    gauges.push(GaugeInfo {
        line_index: lines.len(), ratio: done / total,
        label: format!("{:.0}/{:.0}", done, total),
        fill_color, text_width, right_aligned: false,
    });

    let style = if selected {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    lines.push(Line::from(Span::styled(line_text, style)));
}

/// One discovered flaw, worded for its kind: a reactor's "engine loss"
/// is a shutdown and its degradation is lost power.
pub(super) fn flaw_line(flaw: &Flaw, kind: ProjectKind) -> Line<'static> {
    let consequence = match kind {
        ProjectKind::Reactor => flaw.consequence.reactor_short_label(),
        _ => flaw.consequence.short_label(),
    };
    Line::from(Span::styled(
        format!("        ▲ {}: {} ({})", flaw.description, consequence, format_flaw_rate(flaw)),
        Style::default().fg(Color::Red),
    ))
}

/// The discovered flaws of a project, under a count header. Nothing
/// when none are discovered — hidden flaws stay hidden.
pub(super) fn push_flaw_lines(lines: &mut Vec<Line<'static>>, flaws: &[Flaw], kind: ProjectKind) {
    let discovered = flaws.iter().filter(|f| f.discovered).count();
    if discovered == 0 {
        return;
    }
    lines.push(Line::from(format!("      Flaws: {} discovered", discovered)));
    for flaw in flaws.iter().filter(|f| f.discovered) {
        lines.push(flaw_line(flaw, kind));
    }
}

/// Improvements: actualised ✓ in green first, then pending ★ in cyan.
pub(super) fn push_improvement_lines<K: std::fmt::Display>(lines: &mut Vec<Line<'static>>, improvements: &[Improvement<K>]) {
    for imp in improvements.iter().filter(|i| i.actualized) {
        lines.push(Line::from(Span::styled(
            format!("        ✓ {}: {}", imp.description, imp.kind),
            Style::default().fg(Color::Green),
        )));
    }
    for imp in improvements.iter().filter(|i| !i.actualized) {
        lines.push(Line::from(Span::styled(
            format!("        ★ {}: {} (pending revision)", imp.description, imp.kind),
            Style::default().fg(Color::Cyan),
        )));
    }
}

/// The technology deficiencies still on a project, with how the fight
/// against each is going.
pub(super) fn push_tech_deficiency_lines<D: Designable>(
    lines: &mut Vec<Line<'static>>,
    project: &DesignProject<D>,
    technologies: &[crate::technology::Technology],
) {
    if project.tech_deficiency_ids.is_empty() {
        return;
    }
    let Some(tech) = project.technology_id
        .and_then(|id| technologies.iter().find(|t| t.id == id))
    else {
        return;
    };
    lines.push(Line::from(format!("      Tech deficiencies ({}):", tech.name)));
    for def_id in &project.tech_deficiency_ids {
        let Some(def) = tech.deficiencies.iter().find(|d| d.id == *def_id) else { continue };
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

/// The tail every project pane ends with: the key hints, the bordered
/// block, and the gauges drawn over the rows.
pub(super) fn render_project_pane(
    frame: &mut Frame, area: Rect, border_style: Style, tab: Tab,
    has_selection: bool, mut lines: Vec<Line<'static>>, gauges: Vec<GaugeInfo>,
) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        hint_line_for(crate::ui::keys::for_tab(tab), has_selection, area),
        Style::default().fg(Color::Cyan),
    )));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(format!(" {} ", tab.name()));
    frame.render_widget(Paragraph::new(lines).block(block), area);
    render_gauges(frame, area, &gauges);
}

pub(super) fn draw_engines_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let visible: Vec<&crate::engine_project::EngineProject> =
        company.visible_engine_projects().map(|(_, p)| p).collect();

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(format!("  Engine Projects ({})", visible.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    if visible.is_empty() {
        lines.push(Line::from("  No engine projects yet. Design one inside a rocket."));
    }

    for (i, project) in visible.iter().enumerate() {
        let selected = i == app.selected_item;
        push_project_row(&mut lines, &mut gauges, project, selected, &app.game.balance, "");
        if !selected {
            continue;
        }

        // Propellant display with 2 sig figs
        let prop_str: Vec<String> = project.design.propellant_mix.iter()
            .map(|f| format!("{} {:.0}%", f.propellant.display_name(), f.mass_fraction * 100.0))
            .collect();
        // One project, two bells: show both so the player can see
        // what an upper stage would gain before committing a design.
        let isp_str = if project.has_nozzle_choice() {
            format!("{:.0}s SL / {:.0}s vac",
                project.design_variant(false).isp_s,
                project.design_variant(true).isp_s)
        } else {
            format!("{:.0}s vac", project.design.isp_s)
        };
        lines.push(Line::from(format!(
            "      {}  {}  {}  {}",
            project.design.cycle.display_name(),
            prop_str.join(" / "),
            format_thrust_n(project.design.thrust_n),
            isp_str,
        )));
        let power_str = if project.design.power_draw_w > 0.0 {
            format!("    Power: {}", format_power_w(project.design.power_draw_w))
        } else {
            String::new()
        };
        lines.push(Line::from(format!(
            "      Mass: {}    Teams: {}    Scale: {:.2}x    Auto-revise: {}{}",
            format_kg(project.design.mass_kg),
            project.teams_assigned,
            project.spec.scale,
            if project.auto_revise { "on" } else { "off" },
            power_str,
        )));

        // Show inventory count for engines in Testing or later
        if matches!(project.status, DesignStatus::Testing { .. }) {
            let source = EngineSource::PlayerDesign(project.project_id);
            let count = company.manufacturing.inventory.engine_count(source);
            lines.push(Line::from(format!("      Built engines: {}", count)));
        }

        push_flaw_lines(&mut lines, &project.flaws, ProjectKind::Engine);
        push_improvement_lines(&mut lines, &project.improvements);
        push_tech_deficiency_lines(&mut lines, project, &app.game.technologies);
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
            for flaw in ce.flaws.iter().filter(|f| f.discovered) {
                lines.push(flaw_line(flaw, ProjectKind::Engine));
            }
        }
    }

    render_project_pane(
        frame, area, border_style, Tab::Engines, !visible.is_empty(), lines, gauges,
    );
}

pub(super) fn draw_reactors_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let visible: Vec<&crate::reactor_project::ReactorProject> =
        company.visible_reactor_projects().map(|(_, p)| p).collect();

    let mut lines: Vec<Line<'static>> = vec![
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

    for (i, project) in visible.iter().enumerate() {
        let selected = i == app.selected_item;
        push_project_row(&mut lines, &mut gauges, project, selected, &app.game.balance, "");
        if !selected {
            continue;
        }

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
            "      Teams: {}  NRE: {}",
            project.teams_assigned, format_money(project.nre_cost),
        )));
        // Testing progress (once past design).
        if matches!(project.status, DesignStatus::Testing { .. } | DesignStatus::Revising { .. }) {
            lines.push(Line::from(format!("      Testing: {}", project.testing_level(&app.game.balance))));
        }

        push_flaw_lines(&mut lines, &project.flaws, ProjectKind::Reactor);
        push_improvement_lines(&mut lines, &project.improvements);
        push_tech_deficiency_lines(&mut lines, project, &app.game.technologies);
    }

    render_project_pane(
        frame, area, border_style, Tab::Reactors, !visible.is_empty(), lines, gauges,
    );
}

pub(super) fn draw_rockets_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    // Retired designs are hidden here, so the count, the empty message
    // and the selection all have to work off the visible list — the
    // selection indexes *this*, not `rocket_projects`.
    let visible: Vec<&rocket_project::RocketProject> =
        company.visible_rocket_projects().map(|(_, p)| p).collect();
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(format!("  Rocket Projects ({})", visible.len())),
        Line::from("  ─────────────────────────────────────────────"),
    ];
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    if visible.is_empty() {
        lines.push(Line::from("  No rocket projects yet. Press [N] to start a new design."));
    }

    for (i, project) in visible.iter().enumerate() {
        let selected = i == app.selected_item;

        let auto_target = company.auto_build_targets.get(&project.project_id).copied().unwrap_or(0);
        let auto_suffix = if !selected && auto_target > 0 {
            format!("  [A:{}]", auto_target)
        } else {
            String::new()
        };
        // Nominal build time: the design's critical path with one team on
        // every part, so two designs can be compared without reference to
        // how busy the floor happens to be.
        let build_days = company.nominal_build_days(project, &app.game.balance);
        let extra = format!("  build ~{:.0}d{}", build_days, auto_suffix);
        push_project_row(&mut lines, &mut gauges, project, selected, &app.game.balance, &extra);
        if !selected {
            continue;
        }

        let total_stages: u32 = project.design.stage_groups.iter()
            .map(|g| g.len() as u32).sum();
        let total_engines: u32 = project.design.stage_groups.iter()
            .flat_map(|g| g.iter())
            .map(|s| s.engine_count)
            .sum();
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
                if let Some(entry) = seen_engines.iter_mut().find(|(k, _)| k == &key) {
                    entry.1 += stage.engine_count;
                } else {
                    seen_engines.push((key, stage.engine_count));
                }
            }
        }
        let engine_list: Vec<String> = seen_engines.iter()
            .map(|(name, count)| format!("{}x{}", count, name))
            .collect();
        lines.push(Line::from(format!("      Engines: {}", engine_list.join(", "))));

        // Initial acceleration at takeoff: stage 0, full propellant,
        // 0 payload, 1 AU.
        let avail_power = project.design.power_for_engines_w(1.0);
        let initial_thrust = project.design.group_effective_thrust_n(0, avail_power);
        let initial_mass = project.design.total_mass_kg();
        let initial_accel = if initial_mass > 0.0 { initial_thrust / initial_mass } else { 0.0 };
        // Usable Δv is the planner's figure: vacuum less the ascent's
        // gravity loss, the number the contract colours are judged by.
        let usable_dv = rocket::DesignPerformance::compute(&project.design, 0.0, "earth_surface")
            .total_planner_dv();
        lines.push(Line::from(format!(
            "      Total mass: {:.0} kg    Usable dV: {:.0} m/s (0 payload)    Initial accel: {}",
            project.design.total_mass_kg(),
            usable_dv,
            format_accel(initial_accel),
        )));

        // Show payload table for destinations served by active markets
        // (or the LEO/MEO/GTO/GEO fallback when none are active yet).
        let dests = relevant_destinations(&app.game);
        let table = app.game.payload_table(&project.design, "earth_surface", &dests);
        if !table.is_empty() {
            lines.push(Line::from("      Max payload:"));
            for row in &table {
                let (note, style) = if row.survives {
                    ("", Style::default())
                } else {
                    ("  ▲ no power to reach it", Style::default().fg(Color::Red))
                };
                lines.push(Line::from(Span::styled(
                    format!("        {:20} {:>8}{}", row.destination, format_mass(row.max_payload_kg), note),
                    style,
                )));
            }
        }

        push_flaw_lines(&mut lines, &project.flaws, ProjectKind::Rocket);

        // Inventory count
        let built = company.manufacturing.inventory.rocket_count(project.project_id);
        if built > 0 {
            lines.push(Line::from(format!("      Built rockets: {}", built)));
        }

        // Auto-build target
        let auto_revise = if project.auto_revise { "on" } else { "off" };
        if auto_target > 0 {
            lines.push(Line::from(format!(
                "      Auto-build: {}    Auto-revise: {}", auto_target, auto_revise)));
        } else {
            lines.push(Line::from(format!(
                "      Auto-build: off    Auto-revise: {}", auto_revise)));
        }
    }

    render_project_pane(
        frame, area, border_style, Tab::Rockets, !visible.is_empty(), lines, gauges,
    );
}

pub(super) fn draw_manufacturing_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let company = &app.game.player_company;
    let mfg = &company.manufacturing;
    let mut lines = vec![
        Line::from("  Manufacturing"),
        Line::from("  ─────────────────────────────────────────────"),
        Line::from(format!(
            "  Mfg teams: {} ({} unassigned)",
            company.manufacturing_teams.len(),
            company.unassigned_manufacturing_team_count(),
        )),
    ];
    let mut gauges: Vec<GaugeInfo> = Vec::new();

    let rows = company.manufacturing_display_order();

    if rows.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("  Orders:"));
        lines.push(Line::from("    No manufacturing orders."));
    }

    // Two headers, emitted as the rush block starts and ends. Rushed
    // orders appear only under "Rush jobs", never twice — the selection
    // indexes these rows, so one order has to mean one row.
    let mut header_drawn = (false, false);
    for (row_index, row) in rows.iter().enumerate() {
        if row.rushed && !header_drawn.0 {
            header_drawn.0 = true;
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Rush jobs:",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
        }
        if !row.rushed && !header_drawn.1 {
            header_drawn.1 = true;
            lines.push(Line::from(""));
            lines.push(Line::from("  Orders:"));
        }

        let order = &mfg.orders[row.order_index];
        let selected = row_index == app.selected_item;
        let marker = if selected { "▶" } else { " " };

        let status_str = if order.waiting_for_prerequisites {
            format!("Waiting  Teams: {}", order.teams_assigned)
        } else {
            format!("Teams: {}", order.teams_assigned)
        };

        // Indent by depth: integration, then the stages feeding it, then
        // the engine builds feeding those.
        let indent = "    ".repeat(row.depth as usize);
        let line_text = format!(
            "    {} [{}] {}{} \"{}\"  {}",
            marker, row_index + 1, indent,
            order.type_label(), order.display_name(), status_str,
        );
        let text_width = line_text.chars().count() as u16;

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
        } else if row.rushed {
            Style::default().fg(Color::Yellow)
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
        hint_line_for(
            crate::ui::keys::for_tab(Tab::Manufacturing), true, area,
        ),
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

pub(super) fn draw_launches_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let game = &app.game;
    let rockets = crate::ui::ready_rockets_in_display_order(&game.player_company);

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
                    let leo = game.payload_capability(&rp.design, "earth_surface", "leo");
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
                        .is_some_and(|ss| ss.iter().any(|s| s.attached));
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
                                .is_some_and(|s| s.attached);
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
                        .is_some_and(|ss| ss.iter().any(|s| s.attached));
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
                                .is_some_and(|s| s.attached);
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

pub(super) fn draw_finance_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
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
                    month_name.month_name(),
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

            let name = fit(&rp.design.name, 18);

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

            let name = fit(&ep.design.name, 18);

            lines.push(Line::from(format!(
                "  {:<18} {:>12} {:>12} {:>12} {:>5}",
                name, format_money(ep.nre_cost), avg_str, marginal_str, built
            )));
        }

        // Contracted engines: flat per-unit cost, no NRE/avg.
        for ce in &company.contracted_engines {
            let built = *company.contracted_engine_build_counts
                .get(&ce.id).unwrap_or(&0);
            let name = fit(&ce.design.name, 18);
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

pub(super) fn draw_events_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
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
