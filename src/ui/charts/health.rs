//! Errors, drops, FIFO errors, and collisions per second for each interface, averaged over the
//! window, plus the share of frames that had errors, color-coded.

use std::iter;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Style, Stylize},
    widgets::{Cell, Row, Table, Widget},
};

use crate::{
    history::{History, InterfaceHistory, as_f64},
    model::Counter,
    ui::charts::{NAME_WIDTH, is_gone, last_read, right_aligned, rows_block},
};

/// Every interface's health, in the interface list's order.
pub struct HealthTable<'a> {
    history: &'a History,
}

impl<'a> HealthTable<'a> {
    #[must_use]
    pub const fn new(history: &'a History) -> Self {
        Self { history }
    }
}

/// An interface's problems over the window: per-second averages, plus the error ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Health {
    /// Receive plus transmit errors.
    pub errors: f64,
    /// Receive plus transmit drops.
    pub drops: f64,
    /// Receive plus transmit FIFO errors.
    pub fifo: f64,
    /// Transmit collisions.
    pub collisions: f64,
    /// Errors as a share of all frames, good and bad, or `None` if no frames moved.
    pub error_ratio: Option<f64>,
}

impl Health {
    /// Averages over the samples in the window, or `None` until they cover some time.
    #[must_use]
    pub fn of(history: &InterfaceHistory) -> Option<Self> {
        let span = history.span_secs().filter(|&span| span > 0.0)?;
        let total = |counters: &[Counter]| {
            counters
                .iter()
                .map(|&counter| history.window_total(counter))
                .fold(0, u64::saturating_add)
        };
        let per_sec = |counters: &[Counter]| as_f64(total(counters)) / span;
        // rx_packets and tx_packets count only good frames, so the bad ones are added back.
        let errors = total(&[Counter::RxErrors, Counter::TxErrors]);
        let frames = errors.saturating_add(total(&[Counter::RxPackets, Counter::TxPackets]));
        Some(Self {
            errors: as_f64(errors) / span,
            drops: per_sec(&[Counter::RxDropped, Counter::TxDropped]),
            fifo: per_sec(&[Counter::RxFifo, Counter::TxFifo]),
            collisions: per_sec(&[Counter::TxCollisions]),
            error_ratio: (frames > 0).then(|| as_f64(errors) / as_f64(frames)),
        })
    }
}

/// How worrying an interface's error ratio is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// No frames moved, so there's no ratio.
    Idle,
    /// No errors.
    Clean,
    /// Some errors, on less than [`BAD_RATIO`] of frames.
    Warn,
    /// Errors on at least [`BAD_RATIO`] of frames.
    Bad,
}

/// Errors on 0.1% of frames point at a bad cable, port, or driver. A healthy wired link sees
/// almost none.
pub const BAD_RATIO: f64 = 0.001;

impl Severity {
    /// The severity of an error ratio from [`Health::of`].
    #[must_use]
    pub fn of(error_ratio: Option<f64>) -> Self {
        match error_ratio {
            None => Self::Idle,
            Some(ratio) if ratio <= 0.0 => Self::Clean,
            Some(ratio) if ratio < BAD_RATIO => Self::Warn,
            Some(_) => Self::Bad,
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Idle => Style::new().dark_gray(),
            Self::Clean => Style::new().green(),
            Self::Warn => Style::new().yellow(),
            Self::Bad => Style::new().red().bold(),
        }
    }
}

/// Fits the headers, and rates up to "999999.99".
const COLUMN_WIDTH: u16 = 9;

impl Widget for HealthTable<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header = Row::new([
            Cell::from("interface"),
            right_aligned("errors/s"),
            right_aligned("drops/s"),
            right_aligned("fifo/s"),
            right_aligned("colls/s"),
            right_aligned("err ratio"),
        ])
        .bold();
        let widths = iter::once(Constraint::Length(NAME_WIDTH))
            .chain(iter::repeat_n(Constraint::Length(COLUMN_WIDTH), 5));
        let last_read = last_read(self.history);
        let rows = self.history.iter().map(|(name, history)| {
            let health = Health::of(history);
            row(name, health, is_gone(history, last_read))
        });
        Table::new(rows, widths)
            .header(header)
            .block(rows_block(
                "errors & drops",
                self.history.iter().count(),
                area,
            ))
            .render(area, buf);
    }
}

/// One interface's row. Zero rates are dimmed so the others stand out, and the error ratio takes
/// its severity's color.
fn row(name: &str, health: Option<Health>, gone: bool) -> Row<'_> {
    if gone {
        // Its window stopped when it vanished, so nothing in it is current.
        return Row::new([Cell::from(name), right_aligned("gone")]).dark_gray();
    }
    let Some(health) = health else {
        // Nothing to average until a second sample arrives.
        return Row::new([Cell::from(name)]).dark_gray();
    };
    let rate = |per_sec: f64| {
        let style = if per_sec > 0.0 {
            Style::new().yellow()
        } else {
            Style::new().dark_gray()
        };
        right_aligned(format!("{per_sec:.2}")).style(style)
    };
    let severity = Severity::of(health.error_ratio);
    Row::new([
        Cell::from(name),
        rate(health.errors),
        rate(health.drops),
        rate(health.fifo),
        rate(health.collisions),
        right_aligned(percent(health.error_ratio)).style(severity.style()),
    ])
}

/// The error ratio as a percentage: `"–"` without one, and `"<0.01%"` for a tiny one that would
/// otherwise print as a misleading `"0.00%"`.
fn percent(ratio: Option<f64>) -> String {
    match ratio {
        None => "–".to_owned(),
        Some(ratio) if ratio > 0.0 && ratio < 0.0001 => "<0.01%".to_owned(),
        Some(ratio) => format!("{:.2}%", ratio * 100.0),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;
    use rstest::rstest;

    use super::*;
    use crate::ui::charts::test_support::{render_lines, stats};

    /// eth0 read twice, `secs` apart, with these counters grown in between.
    fn eth0(secs: f64, growth: &[(Counter, u64)]) -> History {
        let mut history = History::new(10);
        history.record(0.0, stats("eth0", &[]));
        history.record(secs, stats("eth0", growth));
        history
    }

    fn health(history: &History) -> Option<Health> {
        Health::of(history.get("eth0").expect("eth0 was recorded"))
    }

    #[rstest]
    fn rates_are_averages_over_the_window() {
        let history = eth0(
            2.0,
            &[
                (Counter::RxErrors, 4),
                (Counter::TxErrors, 2),
                (Counter::RxDropped, 7),
                (Counter::TxDropped, 3),
                (Counter::RxFifo, 1),
                (Counter::TxFifo, 1),
                (Counter::TxCollisions, 8),
                (Counter::RxPackets, 1_000),
                (Counter::TxPackets, 994),
            ],
        );
        let expected = Health {
            errors: 3.0,
            drops: 5.0,
            fifo: 1.0,
            collisions: 4.0,
            // 6 bad frames out of 6 + 1,994.
            error_ratio: Some(0.003),
        };
        assert_eq!(health(&history), Some(expected));
    }

    #[rstest]
    fn nothing_to_average_from_one_sample() {
        let mut history = History::new(10);
        history.record(0.0, stats("eth0", &[]));
        assert_eq!(health(&history), None);
    }

    #[rstest]
    fn no_frames_means_no_ratio() {
        let quiet = Health {
            errors: 0.0,
            drops: 0.0,
            fifo: 0.0,
            collisions: 0.0,
            error_ratio: None,
        };
        assert_eq!(health(&eth0(1.0, &[])), Some(quiet));
    }

    #[rstest]
    #[case::idle(None, Severity::Idle)]
    #[case::clean(Some(0.0), Severity::Clean)]
    #[case::just_under_bad(Some(0.000_9), Severity::Warn)]
    #[case::bad(Some(BAD_RATIO), Severity::Bad)]
    #[case::all_bad(Some(1.0), Severity::Bad)]
    fn severity_follows_the_error_ratio(#[case] ratio: Option<f64>, #[case] expected: Severity) {
        assert_eq!(Severity::of(ratio), expected);
    }

    #[rstest]
    #[case::none(None, "–")]
    #[case::zero(Some(0.0), "0.00%")]
    #[case::tiny(Some(0.000_05), "<0.01%")]
    #[case::smallest_shown(Some(0.000_1), "0.01%")]
    #[case::some(Some(0.003), "0.30%")]
    #[case::all(Some(1.0), "100.00%")]
    fn percent_never_rounds_errors_away(#[case] ratio: Option<f64>, #[case] expected: &str) {
        assert_eq!(percent(ratio), expected);
    }

    #[rstest]
    fn nothing_to_average_from_samples_taken_together() {
        let history = eth0(0.0, &[(Counter::RxErrors, 4)]);
        assert_eq!(health(&history), None);
    }

    /// The color where `text` first appears on line `y` of `buf`.
    fn color_of(buf: &Buffer, y: u16, text: &str) -> Color {
        let row: Vec<&str> = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        let wanted: Vec<String> = text.chars().map(String::from).collect();
        let x = row
            .windows(wanted.len())
            .position(|cells| cells.iter().eq(wanted.iter()))
            .unwrap_or_else(|| panic!("{text:?} isn't on line {y}: {row:?}"));
        buf[(u16::try_from(x).expect("narrow"), y)].fg
    }

    #[rstest]
    fn colors_rates_and_the_ratio_by_how_bad_they_are() {
        let history = eth0(2.0, &[(Counter::RxErrors, 6), (Counter::RxPackets, 1_994)]);
        let area = Rect::new(0, 0, 70, 4);
        let mut buf = Buffer::empty(area);
        HealthTable::new(&history).render(area, &mut buf);
        let row: String = (0..area.width).map(|x| buf[(x, 2)].symbol()).collect();
        assert!(row.starts_with("│eth0"), "{row:?}");
        // 6 errors in 2 seconds, and nothing else went wrong.
        assert_eq!(color_of(&buf, 2, "3.00"), Color::Yellow);
        assert_eq!(color_of(&buf, 2, "0.00"), Color::DarkGray);
        // 6 bad frames of 2,000 is 0.3%, past the 0.1% that counts as bad.
        assert_eq!(color_of(&buf, 2, "0.30%"), Color::Red);
    }

    #[rstest]
    fn a_vanished_interface_says_so() {
        let mut history = eth0(1.0, &[]);
        history.record(0.0, stats("veth0", &[]));
        let lines = render_lines(HealthTable::new(&history), 70, 5);
        assert!(!lines[2].contains("gone"), "{lines:#?}");
        assert!(lines[3].starts_with("│veth0"), "{lines:#?}");
        assert!(lines[3].contains("gone"), "{lines:#?}");
    }
}
