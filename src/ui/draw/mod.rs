//! Drawing. `draw` paints the whole frame; the tabs, the contract table,
//! the rocket designer, the modals and the editors each have a file, and
//! `format` holds the number formatters (17_REFACTOR.md E2).

mod contracts;
mod designer;
mod editors;
mod format;
mod modals;
mod tabs;
mod widgets;

use contracts::*;
use designer::*;
use editors::*;
use format::*;
use modals::*;
use tabs::*;
use widgets::*;

#[cfg(test)]
pub use contracts::test_support;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph};
use crate::calendar::GameDate;
use crate::contract::{self, Contract};
use crate::engine_project::EngineSource;
use crate::game_state::GameState;
use crate::manufacturing::ManufacturingOrderType;
use crate::project::{DesignProject, DesignStatus, Designable, Improvement, ProjectKind};
use crate::rocket_project;
use crate::rocket_perf;
use crate::event::EventImportance;
use crate::flaw::{Flaw, FlawTrigger};
use crate::launch::LaunchOutcome;
use crate::location::DELTA_V_MAP;
use crate::resources::format_money;
use crate::rocket;
use crate::ui::{App, DesignerSubMode, FocusedPane, InputMode, RocketDesignerState, Tab};

/// Deduplicated list of destinations served by the player's currently-active
/// markets — including markets that haven't generated a contract this month.
/// Falls back to the basic Earth-orbit set (LEO, MEO, GTO, GEO) when no
/// markets are active yet.
pub(super) fn relevant_destinations(game: &crate::game_state::GameState) -> Vec<&str> {
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
        dests.extend(["leo", "sso", "meo", "gto", "geo"]);
    }
    dests
}

/// Draw the entire application frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // The rocket designer replaces the full UI, and whatever it has
    // opened over itself — its pickers, the power editor, its help, and
    // the engine editor reached from its picker — draws over the
    // designer, not over the tabs behind it.
    match &app.input_mode {
        InputMode::RocketDesigner { state, sub } => {
            draw_rocket_designer_full(frame, app, state, size);
            if !matches!(sub, DesignerSubMode::Main) {
                draw_modal(frame, app, size);
            }
            return;
        }
        InputMode::EngineEditor { state: Some(state), .. }
        | InputMode::EngineEditorField { state: Some(state), .. } => {
            draw_rocket_designer_full(frame, app, state, size);
            draw_modal(frame, app, size);
            return;
        }
        _ => {}
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

pub(super) fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_main_area(frame: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let highlight_style = if app.focused_pane == FocusedPane::Sidebar {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    let items: Vec<ListItem> = app.tabs().into_iter().map(|tab| {
        let style = if tab == app.active_tab {
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

pub(super) fn draw_content(frame: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_overview(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let game = &app.game;

    // Next steps go first: on a short terminal the stats block below is
    // tall enough to push them off the bottom, and they matter most to
    // exactly the player who can't scroll past them yet. The panel
    // disappears on its own once the company is running.
    let mut lines: Vec<Line> = Vec::new();
    let steps = crate::ui::next_steps::next_steps(game);
    if !steps.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Next steps",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        for s in &steps {
            lines.push(Line::from(vec![
                Span::styled("    → ", Style::default().fg(Color::Cyan)),
                Span::raw(s.text.clone()),
                Span::styled(
                    format!("  ({} tab, [{}])", s.tab, s.key),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines.extend(vec![
        Line::from(format!("  Company:  {}", game.player_company.name)),
        Line::from(format!("  Founded:  {}", game.start_date)),
        Line::from(format!("  Today:    {}", game.date)),
        Line::from(format!("  Elapsed:  {} days", game.elapsed_days())),
        Line::from(""),
        Line::from(format!("  Money:    {}", format_money(game.player_company.money))),
        Line::from(""),
        Line::from(format!("  Eng. teams:      {}", game.player_company.team_count())),
        Line::from(format!("  Mfg. teams:      {}", game.player_company.manufacturing_teams.len())),
        // Visible counts, not raw vec lengths: these are "what am I
        // working on", and a retired design is not that.
        Line::from(format!("  Engine projects: {}",
            game.player_company.visible_engine_projects().count())),
        Line::from(format!("  Rocket projects: {}",
            game.player_company.visible_rocket_projects().count())),
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
    ]);

    // Only while something is happening — peacetime draws no line, so the
    // pane looks exactly as it did before in the ~74% of worlds that never
    // see a war.
    if let Some(headline) = game.geopolitics.headline() {
        lines.push(Line::from(Span::styled(
            format!("  World:           {headline}"),
            Style::default().fg(Color::Rgb(255, 100, 0)).add_modifier(Modifier::BOLD),
        )));
    }

    lines.extend([
        Line::from(""),
        Line::from(format!("  Seed:  {}", game.seed.seed())),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Overview ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Background used to mark the selected row in list views. Selection
/// deliberately owns a channel no semantic colour uses, so it can never
/// be confused with a readiness or status colour.
pub const SELECTION_BG: Color = Color::Rgb(48, 48, 72);

pub(super) fn draw_event_feed(frame: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let text = if let Some(ref msg) = app.status_message {
        format!(" {} ", msg)
    } else if matches!(app.input_mode, InputMode::Help { .. } | InputMode::Intro) {
        " Any key closes this ".to_string()
    } else if !matches!(app.input_mode, InputMode::Normal) {
        " [Enter] Confirm  [Esc] Cancel  [↑↓] Select ".to_string()
    } else {
        // Was hand-written, and had drifted: it never mentioned F12,
        // so the bug-report key was reachable only through `?`.
        hint_line_for(crate::ui::keys::GLOBAL, true, area)
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

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
