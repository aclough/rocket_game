//! Shared widgets: the inline gauges the manufacturing tab and the
//! editors draw over their text lines.

use super::*;

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
