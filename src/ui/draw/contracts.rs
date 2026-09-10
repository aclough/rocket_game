//! The contracts tab: readiness colours, the column layout that fits
//! the pane, and the row renderer the game-state tests check.

use super::*;

/// How ready the player is to fulfill a contract.
pub(super) enum ContractReadiness {
    /// A built rocket in inventory can deliver the payload.
    Ready,
    /// A capable design exists but no rocket is built yet.
    NeedsBuild,
    /// No design (Testing or later) can deliver the required payload.
    Impossible,
}

/// Column widths for the contracts table, sized to the pane.
///
/// The name column absorbs whatever space is left over; the
/// bid-close column is the first thing dropped when the pane is
/// narrow (only solicitations use it, and their bid date is repeated
/// in the detail modal). Everything else is fixed so that rows line
/// up regardless of contract-name length — the whole point of the
/// table.
#[derive(Debug, Clone, Copy)]
pub(super) struct ContractCols {
    pub(super) name: usize,
    pub(super) show_bid_close: bool,
}

impl ContractCols {
    /// Marker + the fixed columns, including the single space that
    /// separates each one. Name and the trailing flag column are the
    /// only variable parts.
    const MARKER: usize = 2;
    const DEST: usize = 6;
    const PAYLOAD: usize = 9;   // "123456 kg"
    const VALUE: usize = 10;    // "bid $12.5M"
    const DATE: usize = 12;     // "Dec 31, 2001" — the longest GameDate
    const LEFT: usize = 5;      // "365d", or "over" once the date has passed
    const FLAGS: usize = 5;     // " ▲rep"
    const MIN_NAME: usize = 10;
    /// Past this the name column is just whitespace; leave the rest of
    /// a wide terminal empty rather than letting the table sprawl.
    const MAX_NAME: usize = 30;

    fn for_width(pane_width: u16) -> Self {
        let inner = (pane_width as usize).saturating_sub(2); // block borders
        // marker + name + (sep+dest) + (sep+payload) + (sep+value)
        // + (sep+bid_close) + (sep+deadline) + flags
        let fixed_narrow = Self::MARKER + (1 + Self::DEST) + (1 + Self::PAYLOAD)
            + (1 + Self::VALUE) + (1 + Self::DATE) + (1 + Self::LEFT) + Self::FLAGS;
        let fixed_wide = fixed_narrow + 1 + Self::DATE;

        let wide_name = inner.saturating_sub(fixed_wide);
        if wide_name >= 16 {
            Self { name: wide_name.min(Self::MAX_NAME), show_bid_close: true }
        } else {
            Self {
                name: inner.saturating_sub(fixed_narrow)
                    .clamp(Self::MIN_NAME, Self::MAX_NAME),
                show_bid_close: false,
            }
        }
    }

    /// Header row matching `contract_row`'s layout.
    fn header(&self) -> String {
        let mut s = format!(
            "  {:<name$} {:<dest$} {:>payload$} {:>value$}",
            "Contract", "Dest", "Payload", "Value",
            name = self.name, dest = Self::DEST,
            payload = Self::PAYLOAD, value = Self::VALUE,
        );
        if self.show_bid_close {
            s.push_str(&format!(" {:<w$}", "Bids close", w = Self::DATE));
        }
        s.push_str(&format!(" {:<w$}", "Deadline", w = Self::DATE));
        s.push_str(&format!(" {:>w$}", "Left", w = Self::LEFT));
        s
    }
}

/// Control hints, indented and clipped to the width they're drawn in.
///
/// Before M5 Task 7 the line was rendered unclipped and the pane cut
/// it off mid-word — which reads as a rendering bug rather than as a
/// terminal that's too narrow.
///
/// `[?] Keys` is appended *after* clipping rather than being part of
/// the line, so it is never what falls off the edge. On a terminal
/// too narrow for the keys, the pointer to the full list is the one
/// thing that has to survive.
pub(super) fn hint_line_for(
    bindings: &[crate::ui::keys::KeyBinding],
    has_selection: bool,
    area: Rect,
) -> String {
    use crate::ui::keys::{HELP_HINT, hint_fragments};

    const GAP: usize = 2;

    // Two columns of border, then the two-space indent.
    let width = (area.width as usize).saturating_sub(4);
    let budget = width.saturating_sub(HELP_HINT.chars().count() + GAP);

    let fragments = hint_fragments(bindings, has_selection);
    let full: usize = fragments.iter().map(|f| f.chars().count()).sum::<usize>()
        + GAP * fragments.len().saturating_sub(1);

    let shown = if full <= budget {
        fragments
    } else {
        // Something has to go, so reserve room for the `…` that says
        // so, and keep whole fragments only.
        let budget = budget.saturating_sub(1 + GAP);
        let mut used = 0;
        let mut shown: Vec<&str> = Vec::new();
        for f in fragments {
            let cost = f.chars().count() + if shown.is_empty() { 0 } else { GAP };
            if used + cost > budget {
                break;
            }
            used += cost;
            shown.push(f);
        }
        shown.push("…");
        shown
    };

    if shown == ["…"] {
        // Too narrow for even one key; the pointer is all that fits.
        return format!("  {HELP_HINT}");
    }
    format!("  {}  {HELP_HINT}", shown.join("  "))
}

/// One row of the contracts table. `selected` only controls the
/// marker — the *style* carries selection (see `contract_style`), so
/// the two signals never contend for the same channel.
pub(super) fn contract_row(
    c: &Contract,
    cols: &ContractCols,
    selected: bool,
    rep_flag: bool,
    today: GameDate,
) -> String {
    let mut s = format!(
        "{}{:<name$} {:<dest$} {:>payload$} {:>value$}",
        if selected { "▶ " } else { "  " },
        fit(&c.name, cols.name),
        fit(contract::destination_short_name(&c.destination), ContractCols::DEST),
        format!("{:.0} kg", c.payload_kg),
        // Solicitation: the price is the player's to name, so the
        // reference payment and budget ceiling stay hidden.
        if c.bid_deadline.is_some() {
            match c.player_bid {
                Some(b) => format!("bid {}", format_money(b)),
                None => "no bid".to_string(),
            }
        } else {
            format_money(c.payment)
        },
        name = cols.name, dest = ContractCols::DEST,
        payload = ContractCols::PAYLOAD, value = ContractCols::VALUE,
    );
    if cols.show_bid_close {
        s.push_str(&format!(
            " {:<w$}",
            c.bid_deadline.map(|d| d.to_string()).unwrap_or_default(),
            w = ContractCols::DATE,
        ));
    }
    s.push_str(&format!(" {:<w$}", c.deadline.to_string(), w = ContractCols::DATE));
    // `days_until` floors at zero, so a passed deadline and a deadline
    // today would both read "0d" — the distinction matters most when
    // you're scanning for what's about to bite.
    s.push_str(&format!(
        " {:>w$}",
        if c.deadline < today { "over".to_string() }
        else { format!("{}d", today.days_until(&c.deadline)) },
        w = ContractCols::LEFT,
    ));
    if rep_flag {
        s.push_str(" ▲rep");
    }
    s
}

/// Style for a contract row. Readiness owns the **foreground** colour;
/// selection owns the **background** plus bold. Before M5 both used
/// `Color::Yellow`, so a selected row was indistinguishable from a
/// borderline (NeedsBuild) one — the two signals are orthogonal and
/// now use orthogonal channels.
pub(super) fn contract_style(
    readiness: Option<ContractReadiness>,
    selected: bool,
    accepted: bool,
) -> Style {
    let base = match readiness {
        // An available contract we can already fly gets no colour;
        // an accepted one goes green (it's committed work that's ready).
        Some(ContractReadiness::Ready) if accepted => Style::default().fg(Color::Green),
        Some(ContractReadiness::Ready) => Style::default(),
        Some(ContractReadiness::NeedsBuild) => Style::default().fg(Color::Yellow),
        Some(ContractReadiness::Impossible) => Style::default().fg(Color::Red),
        None => Style::default(),
    };
    if selected {
        base.bg(SELECTION_BG).add_modifier(Modifier::BOLD)
    } else {
        base
    }
}

/// The colour a contract row gets, from exactly the questions the bid
/// engine asks before it places a bid.
///
/// Both used to answer "can this design serve this contract" separately,
/// and disagreed on the project's status, the payload margin, and trip
/// survival — so a row could sit white, promising a bid, while every rule
/// stayed silent. `project_can_serve` and `free_capable_stock` are now the
/// shared answers.
///
/// `Ready` means a bid would actually follow: some capable design has a
/// rocket built *and uncommitted*. Stock that is already spoken for by an
/// accepted contract or an outstanding bid reads as `NeedsBuild` — build
/// another and the row goes white.
pub(super) fn check_contract_readiness(game: &GameState, contract: &Contract) -> ContractReadiness {
    let capable = game.capable_projects_for(&contract.destination, contract.payload_kg);
    if capable.is_empty() {
        return ContractReadiness::Impossible;
    }
    if game.free_capable_stock(&capable) > 0 {
        ContractReadiness::Ready
    } else {
        ContractReadiness::NeedsBuild
    }
}

pub(super) fn draw_contracts_tab(frame: &mut Frame, app: &App, area: Rect, border_style: Style) {
    let game = &app.game;
    let available = &game.available_contracts;
    let accepted = &game.player_company.active_contracts;
    let rep = game.player_company.reputation.total();
    let cols = ContractCols::for_width(area.width);

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
        lines.push(Line::from(Span::styled(
            cols.header(),
            Style::default().fg(Color::DarkGray),
        )));
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
                let selected = i == app.selected_item;
                let rep_flag = c.bid_deadline.is_some()
                    && rep < 0.8 * market.rep_target;
                lines.push(Line::from(Span::styled(
                    contract_row(c, &cols, selected, rep_flag, game.date),
                    contract_style(
                        Some(check_contract_readiness(game, c)),
                        selected,
                        false,
                    ),
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
                let selected = i == app.selected_item;
                lines.push(Line::from(Span::styled(
                    contract_row(c, &cols, selected, false, game.date),
                    contract_style(None, selected, false),
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
        lines.push(Line::from(Span::styled(
            cols.header(),
            Style::default().fg(Color::DarkGray),
        )));
        let offset = available.len();
        for (i, c) in accepted.iter().enumerate() {
            let idx = offset + i;
            let selected = idx == app.selected_item;
            lines.push(Line::from(Span::styled(
                contract_row(c, &cols, selected, false, game.date),
                contract_style(
                    Some(check_contract_readiness(game, c)),
                    selected,
                    true,
                ),
            )));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Contracts  [B] Bid / Accept  [R] Bid Rules  [P] Programs  [H] History ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}


/// Hooks for testing the row formatters, which are otherwise only
/// reachable through a rendered frame.
#[cfg(test)]
pub mod test_support {
    use super::*;

    /// One contract row as the pane would draw it, at a width wide enough
    /// for every column.
    pub fn contract_row_for_test(c: &Contract, today: GameDate) -> String {
        contract_row(c, &ContractCols::for_width(120), false, false, today)
    }
}
