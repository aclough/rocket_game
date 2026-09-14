//! Shared widgets: the one way a modal is framed and its rows marked
//! (17_4_UI.md F2), and the inline gauges the manufacturing tab and the
//! editors draw over their text lines.

use super::*;

/// Every modal's border and title colour. Pickers used to be Cyan and
/// confirmations Yellow for no stated reason; now one accent.
pub(super) const MODAL_ACCENT: Color = Color::Yellow;

/// The bordered, titled block every modal draws in.
pub(super) fn modal_block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title.into())
        .style(Style::default().fg(MODAL_ACCENT))
}

/// Draw `lines` as a modal titled `title` over `area`.
pub(super) fn render_modal(frame: &mut Frame, area: Rect, title: impl Into<String>, lines: Vec<Line<'static>>) {
    frame.render_widget(Paragraph::new(lines).block(modal_block(title)), area);
}

/// How a list row reads when the cursor is on it: bold in the accent.
/// Selection in the contracts table is a background instead, because
/// there the foreground already carries readiness; modal rows carry
/// nothing else, so the accent is free to mean "this one".
pub(super) fn selected_style(selected: bool) -> Style {
    if selected {
        Style::default().fg(MODAL_ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

/// The key hint at the foot of a modal.
pub(super) fn hint_line(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::default().fg(Color::DarkGray)))
}

/// Minimum gauge width.
pub(super) const MIN_GAUGE_WIDTH: u16 = 12;

/// Fixed gauge width for right-aligned gauges.
pub(super) const RIGHT_GAUGE_WIDTH: u16 = 20;

/// A progress gauge to overlay on a line.
pub(super) struct GaugeInfo {
    pub(super) line_index: usize,
    pub(super) ratio: f64,
    pub(super) label: String,
    pub(super) fill_color: Color,
    /// Column where the text on this line ends (for positioning gauge
    /// after text). Counted in characters, not bytes — these lines carry
    /// multi-byte glyphs (`▶`, the priority markers), and a byte count
    /// pushes the gauge right by one column per extra byte.
    pub(super) text_width: u16,
    /// If true, use fixed-width right-aligned positioning instead of text-adjacent.
    pub(super) right_aligned: bool,
}

/// Render Gauge overlays on top of a paragraph block.
/// `area` is the outer Rect of the containing block (with borders).
pub(super) fn render_gauges(
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

/// Break `text` into lines no wider than `width` characters at word
/// boundaries (a single word longer than `width` stands alone). For
/// modal text that is composed at runtime and cannot be pre-wrapped.
pub(super) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let need = if line.is_empty() { word.chars().count() } else { line.chars().count() + 1 + word.chars().count() };
        if !line.is_empty() && need > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod wrap_tests {
    use super::wrap_words;

    #[test]
    fn wraps_at_words_and_never_past_the_width() {
        let text = "Design a second engine for the upper stage (a different propellant flies higher)";
        let lines = wrap_words(text, 30);
        assert!(lines.iter().all(|l| l.chars().count() <= 30), "{lines:?}");
        assert_eq!(lines.join(" "), text, "nothing lost, nothing added");
        assert_eq!(wrap_words("", 10), vec![String::new()]);
        assert_eq!(wrap_words("supercalifragilistic", 5), vec!["supercalifragilistic".to_string()]);
    }
}
