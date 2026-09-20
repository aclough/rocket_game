//! The engine, reactor and power editor modals.

use super::*;

/// Non-linear engine editor. The cursor walks four rows:
/// 0=Name, 1=Cycle, 2=Preset, 3=Scale. When `text_input` is Some, a
/// sub-modal text/number entry is overlaid (for Name or Scale).
///
/// There is no nozzle row — a project designs an engine *family* and
/// each stage picks its own bell in the rocket designer ([V] there).
pub(super) fn draw_engine_editor_modal(
    frame: &mut Frame,
    app: &App,
    project_id: crate::engine_project::EngineProjectId,
    cursor: usize,
    text_input: Option<(&str, String)>,
    standalone: bool,
    area: Rect,
) {
    let ep = match app.game.player_company.find_engine_project(project_id) {
        Some(ep) => ep,
        None => return,
    };
    let baseline = crate::engine_project::engine_baseline(ep.design.cycle, ep.spec.preset);
    let vacuum_only = baseline.is_some_and(|b| b.vacuum_only);
    let row_count = 4; // Name, Cycle, Preset, Scale
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
        hint_line(format!(" Status: {}{}", ep.status.label(), match &ep.status {
                crate::engine_project::EngineDesignStatus::Proposed { .. } => " (new draft)",
                crate::engine_project::EngineDesignStatus::Testing { .. } => " (read-only)",
                _ => "",
            })),
        Line::from(""),
    ];

    lines.push(Line::from(Span::styled(
        format!(" {} Name:   {}", row_label(0, true), ep.design.name),
        row_style(0),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} Cycle:  {}", row_label(1, true), ep.design.cycle.display_name()),
        row_style(1),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} Preset: {}", row_label(2, true), ep.spec.preset.name()),
        row_style(2),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} Scale:  {:.3}×", row_label(3, true), ep.spec.scale),
        row_style(3),
    )));
    lines.push(Line::from(Span::styled(
        if vacuum_only {
            "   Nozzle: vacuum only".to_string()
        } else {
            "   Nozzle: chosen per stage in the rocket designer".to_string()
        },
        Style::default().fg(Color::DarkGray),
    )));

    // Live + baseline derived stats.
    lines.push(Line::from(""));
    if let Some(b) = baseline {
        lines.push(hint_line(format!(" Baseline ({:?} / {}):  thrust {}  mass {}  Isp {:.0} s",
                ep.design.cycle, ep.spec.preset.name(),
                format_thrust_n(b.thrust_n), format_kg(b.mass_kg), b.isp_ref_s)));
    }
    lines.push(Line::from(format!(
        " Scaled:    thrust {}  mass {}  Isp {}  power {}",
        format_thrust_n(ep.design.thrust_n),
        format_kg(ep.design.mass_kg),
        if vacuum_only {
            format!("{:.0} s", ep.design_variant(true, &app.game.balance.nozzle).isp_s)
        } else {
            format!("{:.0} s SL / {:.0} s vac",
                ep.design_variant(false, &app.game.balance.nozzle).isp_s,
                ep.design_variant(true, &app.game.balance.nozzle).isp_s)
        },
        format_power_w(ep.design.power_draw_w),
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
    let footer = if standalone {
        " [↑↓] Move  [←→] Change  [Enter] Edit text  [D] Done  [Esc] Cancel"
    } else {
        " [↑↓] Move  [←→] Change  [Enter] Edit text  [Esc] Done"
    };
    lines.push(Line::from(Span::styled(footer, Style::default().fg(Color::DarkGray))));

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

    render_modal(frame, area, " Engine Editor ", lines);
}

pub(super) fn draw_reactor_editor_modal(
    frame: &mut Frame,
    app: &App,
    project_id: crate::reactor_project::ReactorProjectId,
    cursor: usize,
    text_input: Option<(&str, String)>,
    area: Rect,
) {
    use crate::reactor::EnrichmentLevel;
    use crate::reactor_project::ReactorDesignStatus;

    let rp = match app.game.player_company.find_reactor_project(project_id) {
        Some(rp) => rp,
        None => return,
    };
    const ROW_COUNT: usize = 3; // Name, Scale, Enrichment
    let cursor = cursor.min(ROW_COUNT - 1);

    let row_label = |row: usize| -> &'static str {
        if cursor == row { "▶" } else { " " }
    };
    let row_style = |row: usize| -> Style {
        if cursor == row {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else { Style::default() }
    };

    let status_label = format!("{}{}", rp.status.label(), match &rp.status {
        ReactorDesignStatus::Proposed { .. } => " (new draft)",
        ReactorDesignStatus::Testing { .. } => " (read-only)",
        _ => "",
    });

    // Enrichment row: list the levels, dim ones still gated by
    // reputation, mark the current pick, and annotate the row's tail
    // with the next reputation gate so the player sees what they're
    // working toward.
    let reputation = app.game.player_company.reputation.total();
    let enrichment_segments: Vec<String> = EnrichmentLevel::ALL.iter().map(|lvl| {
        let mark_l = if *lvl == rp.design.enrichment { "[" } else { " " };
        let mark_r = if *lvl == rp.design.enrichment { "]" } else { " " };
        format!("{}{}{}", mark_l, lvl.display_name(), mark_r)
    }).collect();
    let next_gate_hint = EnrichmentLevel::ALL.iter()
        .find(|lvl| !lvl.available_at(reputation, &app.game.balance.reputation))
        .map(|lvl| format!("(next: {} at {:.0} rep, you have {:.0})",
            lvl.display_name(), lvl.min_reputation(&app.game.balance.reputation), reputation))
        .unwrap_or_else(|| "(all enrichments unlocked)".into());
    let enrichment_row = format!(
        " {} Enrichment: {}  {}",
        row_label(2),
        enrichment_segments.join(" "),
        next_gate_hint,
    );

    let mut lines = vec![
        hint_line(format!(" Status: {}", status_label)),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {} Name:  {}", row_label(0), rp.design.name),
            row_style(0),
        )),
        Line::from(Span::styled(
            format!(" {} Scale: {:.3}×", row_label(1), rp.design.scale),
            row_style(1),
        )),
        Line::from(Span::styled(enrichment_row, row_style(2))),
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
        " [↑↓] Field  [←→] Change  [Enter] Edit text  [D] Done  [Esc] Cancel"
    } else {
        " [↑↓] Field  [←→] Change  [Enter] Edit text  [Esc] Close"
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

    render_modal(frame, area, " Reactor Editor ", lines);
}

pub(super) fn draw_power_editor_modal(
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
    // Header figures read the effective rack — an empty rack still flies the
    // default battery. The Equipped list below deliberately does not: it is
    // the editable set the cursor indexes into, and the default is not
    // something the player can select or remove.
    let supply_w = stage.supply_w(1.0);
    let idle_demand_w = stage.housekeeping_w();
    let engine_draw_w = stage.engine.power_draw_w * stage.engine_count as f64;
    let thrust_demand_w = idle_demand_w + engine_draw_w;
    let battery_kwd: f64 = stage.effective_power_sources().iter()
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
                "  Supply @ 1 AU: {}    Idle demand: {}    Thrust demand: {}    Battery: {:.2} kWd",
                format_power_w(supply_w),
                format_power_w(idle_demand_w),
                format_power_w(thrust_demand_w),
                battery_kwd,
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
    lines.push(hint_line("  ── Equipped ──"));
    if n_equipped == 0 {
        lines.push(hint_line(format!(
                "    (none fitted — flying the default battery, {battery_kwd:.2} kWd, \
                 about {:.0} day of housekeeping)",
                crate::power::DEFAULT_BATTERY_DAYS,
            )));
    }
    for src in &stage.power_sources {
        let mark = if cursor == row { " ▶ " } else { "   " };
        let style = selected_style(cursor == row);
        lines.push(Line::from(Span::styled(
            format!("{}{}", mark, source_summary(src)),
            style,
        )));
        row += 1;
    }
    lines.push(Line::from(""));

    // Player-researched reactors (installable when Testing+).
    if !player_reactors.is_empty() {
        lines.push(hint_line("  ── Player Reactors ──"));
        for rp in &player_reactors {
            let mark = if cursor == row { " ▶ " } else { "   " };
            let style = selected_style(cursor == row);
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
    lines.push(hint_line("  ── Add ──"));
    for preset in presets {
        let mark = if cursor == row { " ▶ " } else { "   " };
        let style = selected_style(cursor == row);
        lines.push(Line::from(Span::styled(
            format!("{}{}", mark, preset.label),
            style,
        )));
        row += 1;
    }
    lines.push(Line::from(""));
    lines.push(hint_line("  [↑↓] Navigate  [Space] Add  [+/-] Resize Panel  [X/Del] Remove  [Esc] Done"));

    render_modal(frame, area, format!(" Power Editor — {} ", stage_label), lines);
}
