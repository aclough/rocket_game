//! The rocket designer, full screen, and its engine picker.

use super::*;

pub(super) fn draw_rocket_designer_full(frame: &mut Frame, app: &App, state: &RocketDesignerState, area: Rect) {
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
        // Was hand-written here, duplicating ROCKET_DESIGNER and free
        // to drift from it. Same keys, now from the table.
        hint_line_for(crate::ui::keys::ROCKET_DESIGNER, true, outer[1])
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

pub(super) fn draw_rocket_designer_content(frame: &mut Frame, app: &App, state: &RocketDesignerState, area: Rect) {
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
    // One performance figure for the mission line and the stats table:
    // the planner's Avail and the table's Eff dV come from the same
    // ascent integration, computed once per frame rather than twice.
    let perf = (!state.stage_groups.is_empty())
        .then(|| rocket::DesignPerformance::compute(&temp_design, state.payload_kg, state.launch_from));
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
            crate::path_planning::MissionPlan::StagingInfeasible { min_required_dv, available_dv } => Line::from(Span::styled(
                format!("  Mission: {} → {}    UNREACHABLE — Δv is there ({:.0} vs {:.0} needed) but no stage can spend it along the route",
                    launch_display, destination_display, available_dv, min_required_dv),
                Style::default().fg(Color::Red),
            )),
            crate::path_planning::MissionPlan::ClassMismatch { .. } => Line::from(Span::styled(
                format!("  Mission: {} → {}    UNREACHABLE — no route exists for this engine type",
                    launch_display, destination_display),
                Style::default().fg(Color::Red),
            )),
            crate::path_planning::MissionPlan::Reachable { path, dv: required_dv } => {
                // The planner's own figure — vacuum less what the
                // ascent costs — so the margin is the one it judged by.
                let available_dv = perf.as_ref().map_or(0.0, |p| p.total_planner_dv());
                let margin = available_dv - required_dv;
                // Reuses the path the planner just found, so the endurance
                // answer costs no extra search — and it is the same check
                // the contract list colours by, so the two agree.
                let power = rocket_perf::trip_power_along(
                    &temp_design, &path, state.payload_kg,
                );
                // Calendar days from launch to arrival. `flight_days`
                // counts the launch day itself, and a same-day run to LEO
                // arrives before the clock moves.
                let eta_days = power.flight_days.saturating_sub(1);
                let color = if margin < 0.0 || !power.survives() { Color::Red }
                    else if margin < 500.0 { Color::Yellow }
                    else { Color::Green };
                let eta_str = if eta_days == 0 { "<1 d".to_string() }
                    else { format!("{} d", eta_days) };
                // Reaching it and surviving the trip are separate failures,
                // so name which one bit.
                let power_note = match power.dark_on_day {
                    Some(day) => format!(
                        "    POWER: dark on day {} of {}",
                        day, power.flight_days,
                    ),
                    None => String::new(),
                };
                Line::from(Span::styled(
                    format!(
                        "  Mission: {} → {}    Req Δv: {:.0}    Avail: {:.0}    Margin: {:+.0} m/s    ETA: {}{}",
                        launch_display, destination_display,
                        required_dv, available_dv, margin, eta_str, power_note,
                    ),
                    Style::default().fg(color),
                ))
            }
        }
    };
    lines.push(mission_line);
    lines.push(Line::from(""));

    let stats: Vec<rocket::StageGroupStats> = perf.map(|p| p.groups).unwrap_or_default();

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
            // Which bell this stage flies. An engine family with no
            // sea-level bell (expander, electric, nuclear thermal, sail)
            // says so, because a player putting one under a first stage
            // has nothing to toggle and should be told why.
            let nozzle = match state.engine_sources.get(gi).and_then(|g| g.get(si)) {
                Some(EngineSource::PlayerDesign(pid)) => {
                    let has_choice = app.game.player_company
                        .find_engine_project(*pid)
                        .is_some_and(|ep| ep.has_nozzle_choice());
                    if !has_choice { " vac-only" }
                    else if stage.engine.is_vacuum_variant() { " vac" }
                    else { " SL" }
                }
                _ => "",
            };
            let engine_label = format!("{}{}{}", stage.engine.name, tag, nozzle);

            // Compute burn time: propellant_mass / (mass_flow_rate * engine_count)
            let burn_str = if stage.engine.is_solar_sail() {
                "   ∞".to_string()
            } else {
                let burn_time_s = stage.burn_time_s();
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
                    let eff_str = if s.is_sail { "     ∞".to_string() }
                        else { format!("{:>6.0}", s.effective_dv()) };
                    let vac_str = if s.is_sail { "       ∞".to_string() }
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

            // Per-stage power summary (compact). Always shown: a stage with
            // an empty rack is not unpowered, it is flying the default
            // battery, and the player should see the kit it is carrying.
            {
                let sources = stage.effective_power_sources();
                let supply: f64 = sources.iter()
                    .map(|p| stage.source_supply_w(p, 1.0)).sum();
                let battery: f64 = sources.iter()
                    .filter_map(|p| match p.kind {
                        crate::power::PowerSourceKind::Battery => Some(p.capacity_kwd),
                        _ => None,
                    }).sum();
                let demand = stage.housekeeping_w();
                let fitted = if stage.power_sources.is_empty() {
                    "default battery".to_string()
                } else {
                    format!("{} src", stage.power_sources.len())
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}      power: {}, {:.0}/{:.0} W @ 1AU, {:.2} kWd",
                        group_indent, fitted, supply, demand, battery,
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
                                "        ▲ Flow separation risk: {:.0}%/engine, Isp penalty: {:.0}%  (exit {:.0} kPa)",
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
        let total_dv_effective: f64 = stats.iter().map(|s| s.effective_dv()).sum();
        let total_dv_vacuum: f64 = stats.iter().map(|s| s.display_vacuum_dv()).sum();
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
        let initial_accel = temp_design.initial_accel_m_s2(state.payload_kg, 1.0);
        lines.push(Line::from(format!(
            "  Initial accel: {}",
            format_accel(initial_accel),
        )));

        // Electrical summary. Read-only for now; editing UI is a follow-up.
        // Compute supply at takeoff (1 AU) and housekeeping demand across
        // attached stages; show whether designs balance.
        let total_housekeeping = temp_design.total_housekeeping_w();
        let total_supply_1au = temp_design.total_power_supply_w(1.0);
        let total_battery_kwd = temp_design.total_battery_kwd();
        // Endurance, not surplus, is what kills a mission: a craft with no
        // generation is fine for a same-day LEO drop and dead on the way to
        // Mars. Quote how long it lasts and flag only the case that cannot
        // survive its own first day.
        let endurance = temp_design.endurance_days(1.0);
        let endurance_text = if endurance.is_infinite() {
            "indefinite".to_string()
        } else {
            format!("{endurance:.1} d")
        };
        let summary = format!(
            "  Power: {:.0} W supply (@ 1 AU)  /  {:.0} W demand  battery: {:.2} kWd  endurance: {}",
            total_supply_1au, total_housekeeping, total_battery_kwd, endurance_text,
        );
        lines.push(Line::from(Span::styled(
            summary,
            if endurance < 1.0 {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )));
        lines.push(Line::from(""));

        // Payload feasibility for destinations served by active markets
        // (or the LEO/MEO/GTO/GEO fallback when none are active yet).
        let dests = relevant_destinations(&app.game);
        let table = app.game.payload_table(&temp_design, state.launch_from, &dests);
        if !table.is_empty() {
            lines.push(Line::from(Span::styled(
                "  Payload Feasibility:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for row in &table {
                // Lifting the mass and arriving with the lights on are
                // separate failures, and a payload figure on its own reads
                // as success — so say which one is missing.
                let (note, style) = if row.survives {
                    ("", Style::default())
                } else {
                    (
                        "  ▲ goes dark en route — add power",
                        Style::default().fg(Color::Red),
                    )
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "    {:24} {:>8}{}",
                        row.destination, format_mass(row.max_payload_kg), note,
                    ),
                    style,
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

pub(super) fn draw_rocket_pick_engine_modal(
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
            format!("  {} {}{}  {}  {:.0}s  {}",
                marker, design.name, tag,
                format_thrust_n(design.thrust_n), design.isp_s,
                format_kg(design.mass_kg)),
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
