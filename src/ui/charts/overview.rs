//! One row per interface: its receive and transmit rates now, and a sparkline of each.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{RenderDirection, Sparkline, Widget},
};

use crate::{
    history::{History, InterfaceHistory},
    model::Counter,
    ui::{
        charts::{NAME_WIDTH, is_gone, last_read, rows_block},
        format,
    },
};

/// Every interface's current rates and their recent trend, with rx in cyan and tx in magenta
/// like the throughput chart.
pub struct OverviewGrid<'a> {
    history: &'a History,
}

impl<'a> OverviewGrid<'a> {
    #[must_use]
    pub const fn new(history: &'a History) -> Self {
        Self { history }
    }
}

/// The widest `format::rate` label, like "1023.9 KiB/s".
const RATE_WIDTH: u16 = 12;

impl Widget for OverviewGrid<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = rows_block("overview", self.history.iter().count(), area);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut rows = inner.rows();
        let Some(header) = rows.next() else {
            return;
        };
        let template = columns(header);
        let [name_header, rx_header, _, tx_header, _] = template;
        Line::from("interface").bold().render(name_header, buf);
        Line::from("rx")
            .bold()
            .cyan()
            .right_aligned()
            .render(rx_header, buf);
        Line::from("tx")
            .bold()
            .magenta()
            .right_aligned()
            .render(tx_header, buf);

        let last_read = last_read(self.history);
        for (row, (name, history)) in rows.zip(self.history.iter()) {
            // Each row reuses the header's columns, one line lower.
            let [name_area, rx_rate, rx_bars, tx_rate, tx_bars] =
                template.map(|column| Rect { y: row.y, ..column });
            let gone = is_gone(history, last_read);
            let rx = Trend::new(history, Counter::RxBytes, rx_bars.width);
            let tx = Trend::new(history, Counter::TxBytes, tx_bars.width);
            // One scale for both directions, so their bars compare.
            let max = rx.bars.iter().chain(&tx.bars).copied().max().unwrap_or(0);
            Line::from(name).render(name_area, buf);
            rx.render(gone, max, Color::Cyan, [rx_rate, rx_bars], buf);
            tx.render(gone, max, Color::Magenta, [tx_rate, tx_bars], buf);
            if gone {
                // Keep its last trend for context, but dimmed.
                buf.set_style(row, Style::new().dark_gray());
            }
        }
    }
}

/// The name, then the rate and sparkline for rx, then the same for tx.
fn columns(row: Rect) -> [Rect; 5] {
    Layout::horizontal([
        Constraint::Length(NAME_WIDTH),
        Constraint::Length(RATE_WIDTH),
        Constraint::Fill(1),
        Constraint::Length(RATE_WIDTH),
        Constraint::Fill(1),
    ])
    .spacing(1)
    .areas(row)
}

/// One direction of one interface: its latest rate, and its recent rates as sparkline bars.
struct Trend {
    /// `None` until there are two samples.
    now: Option<f64>,
    /// Newest first, so the sparkline draws them right to left with the newest at the edge.
    bars: Vec<u64>,
}

impl Trend {
    fn new(history: &InterfaceHistory, counter: Counter, width: u16) -> Self {
        Self {
            now: history.rates(counter).next_back().map(|(_, rate)| rate),
            bars: history
                .rates(counter)
                .rev()
                .take(usize::from(width))
                .map(|(_, rate)| whole(rate))
                .collect(),
        }
    }

    /// Draws the rate label in `rate` and the bars, scaled to `max`, in `bars`.
    fn render(
        &self,
        gone: bool,
        max: u64,
        color: Color,
        [rate, bars]: [Rect; 2],
        buf: &mut Buffer,
    ) {
        let label = if gone {
            Line::from("gone")
        } else {
            self.now.map_or_else(
                || Line::from("–").dark_gray(),
                |now| Line::from(format::rate(now)),
            )
        };
        label.right_aligned().render(rate, buf);
        Sparkline::default()
            .data(&self.bars)
            .max(max)
            .direction(RenderDirection::RightToLeft)
            .style(color)
            .render(bars, buf);
    }
}

/// A rate rounded to whole bytes per second, for a sparkline bar.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "rates are never negative, and a float-to-int `as` saturates"
)]
fn whole(rate: f64) -> u64 {
    rate.round() as u64
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::ui::charts::test_support::{render_lines, stats};

    /// A history fed `(t, rx_bytes)` readings of eth0.
    fn eth0(readings: &[(f64, u64)]) -> History {
        let mut history = History::new(10);
        for &(t, rx_bytes) in readings {
            history.record(t, stats("eth0", &[(Counter::RxBytes, rx_bytes)]));
        }
        history
    }

    fn rx_trend(history: &History, width: u16) -> Trend {
        let eth0 = history.get("eth0").expect("eth0 was recorded");
        Trend::new(eth0, Counter::RxBytes, width)
    }

    #[rstest]
    fn trends_are_newest_first_and_fit_the_width() {
        // rx at 100, 200, then 300 B/s.
        let history = eth0(&[(0.0, 0), (1.0, 100), (2.0, 300), (3.0, 600)]);
        assert_eq!(rx_trend(&history, 2).now, Some(300.0));
        assert_eq!(rx_trend(&history, 2).bars, [300, 200]);
        assert_eq!(rx_trend(&history, 10).bars, [300, 200, 100]);
    }

    #[rstest]
    fn no_rate_before_the_second_sample() {
        let trend = rx_trend(&eth0(&[(0.0, 100)]), 10);
        assert_eq!(trend.now, None);
        assert_eq!(trend.bars, []);
    }

    #[rstest]
    #[case::rounds_down(1.4, 1)]
    #[case::rounds_up(1.5, 2)]
    #[case::saturates(1e30, u64::MAX)]
    fn bars_are_whole_bytes_per_second(#[case] rate: f64, #[case] expected: u64) {
        assert_eq!(whole(rate), expected);
    }

    #[rstest]
    fn renders_a_row_per_interface() {
        let mut history = eth0(&[(0.0, 0), (1.0, 1536)]);
        history.record(1.0, stats("lo", &[]));
        let lines = render_lines(OverviewGrid::new(&history), 80, 6);
        assert!(lines[0].contains(" overview "), "{lines:#?}");
        assert!(lines[1].contains("interface"), "{lines:#?}");
        assert!(lines[2].starts_with("│eth0"), "{lines:#?}");
        assert!(lines[2].contains("1.5 KiB/s"), "{lines:#?}");
        // lo has a single sample, so no rate yet.
        assert!(lines[3].starts_with("│lo"), "{lines:#?}");
        assert!(lines[3].contains('–'), "{lines:#?}");
    }

    #[rstest]
    fn a_vanished_interface_says_so() {
        let mut history = eth0(&[(0.0, 0), (1.0, 100), (2.0, 200)]);
        history.record(0.0, stats("veth0", &[]));
        history.record(1.0, stats("veth0", &[(Counter::RxBytes, 5_000)]));
        let lines = render_lines(OverviewGrid::new(&history), 80, 5);
        assert!(!lines[2].contains("gone"), "{lines:#?}");
        assert!(lines[3].starts_with("│veth0"), "{lines:#?}");
        assert!(lines[3].contains("gone"), "{lines:#?}");
    }

    #[rstest]
    fn says_how_many_interfaces_did_not_fit() {
        let mut history = History::new(10);
        for name in ["lo", "eth0", "wlan0"] {
            history.record(0.0, stats(name, &[]));
        }
        // Room for the header and one row.
        let lines = render_lines(OverviewGrid::new(&history), 60, 4);
        assert!(lines[2].starts_with("│lo"), "{lines:#?}");
        assert!(lines[3].contains("+2 more"), "{lines:#?}");
    }
}
