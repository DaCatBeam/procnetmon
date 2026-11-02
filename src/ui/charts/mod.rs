pub mod health;
pub mod overview;
pub mod throughput;
pub mod totals;

use std::time::Duration;

use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Cell, Paragraph},
};

use crate::{
    history::{History, InterfaceHistory},
    ui::charts::{
        health::HealthTable, overview::OverviewGrid, throughput::ThroughputChart,
        totals::TotalsTable,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    #[default]
    Throughput,
    Overview,
    Totals,
    Health,
}

impl ChartKind {
    pub const ALL: [Self; 4] = [Self::Throughput, Self::Overview, Self::Totals, Self::Health];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Throughput => "throughput",
            Self::Overview => "overview",
            Self::Totals => "session totals",
            Self::Health => "errors & drops",
        }
    }
}

/// Everything a chart may read. Borrowed; built fresh each frame.
pub struct ViewContext<'a> {
    pub history: &'a History,
    pub selected: Option<(&'a str, &'a InterfaceHistory)>,
    pub window: Duration,
}

pub fn render_chart(kind: ChartKind, ctx: &ViewContext<'_>, frame: &mut Frame, area: Rect) {
    match (kind, ctx.selected) {
        (ChartKind::Throughput, Some((name, history))) => frame.render_widget(
            ThroughputChart {
                name,
                history,
                window_secs: ctx.window.as_secs_f64(),
            },
            area,
        ),
        (ChartKind::Overview, _) => frame.render_widget(OverviewGrid::new(ctx.history), area),
        (ChartKind::Totals, _) => frame.render_widget(TotalsTable::new(ctx.history), area),
        (ChartKind::Health, _) => frame.render_widget(HealthTable::new(ctx.history), area),
        (ChartKind::Throughput, None) => {
            frame.render_widget(
                Paragraph::new("waiting for the first sample…").centered(),
                area,
            );
        }
    }
}

/// Linux interface names are at most 15 bytes.
const NAME_WIDTH: u16 = 15;

/// The border around a chart with a one-line header and a row per interface. `title` goes on
/// top, and when some of the `interfaces` don't fit in `area`, the bottom says how many.
fn rows_block(title: &str, interfaces: usize, area: Rect) -> Block<'static> {
    // Two lines of border and one of header.
    let fits = usize::from(area.height.saturating_sub(3));
    let block = Block::bordered().title(Line::from(format!(" {title} ")).bold());
    match interfaces.saturating_sub(fits) {
        0 => block,
        hidden => block.title_bottom(Line::from(format!(" +{hidden} more ")).right_aligned()),
    }
}

/// A right-aligned table cell, for numbers and their headers.
fn right_aligned<'a>(text: impl Into<Text<'a>>) -> Cell<'a> {
    Cell::from(text.into().right_aligned())
}

/// When the newest read happened, once there's been one.
fn last_read(history: &History) -> Option<f64> {
    history
        .iter()
        .filter_map(|(_, interface)| interface.latest())
        .map(|sample| sample.t)
        .reduce(f64::max)
}

/// Whether `interface` was missing from the newest read, like a stopped container's veth. The
/// history keeps it, but its numbers stopped changing.
fn is_gone(interface: &InterfaceHistory, last_read: Option<f64>) -> bool {
    interface
        .latest()
        .zip(last_read)
        .is_some_and(|(sample, last)| sample.t < last)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};
    use rstest::rstest;

    use super::test_support::stats;
    use super::*;
    use crate::test_support::lines;

    #[rstest]
    fn every_chart_has_its_own_title() {
        let titles = ChartKind::ALL.map(ChartKind::title);
        assert_eq!(
            titles,
            ["throughput", "overview", "session totals", "errors & drops"]
        );
    }

    /// Draws `kind` for a history holding eth0, with `selected` picked in the interface list.
    fn draw(kind: ChartKind, selected: bool) -> String {
        let mut history = History::new(10);
        history.record(0.0, stats("eth0", &[]));
        let eth0 = history.get("eth0").expect("recorded");
        let ctx = ViewContext {
            history: &history,
            selected: selected.then_some(("eth0", eth0)),
            window: Duration::from_secs(300),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("a test terminal");
        terminal
            .draw(|frame| render_chart(kind, &ctx, frame, frame.area()))
            .expect("a frame");
        lines(terminal.backend().buffer()).join("\n")
    }

    #[rstest]
    #[case::throughput(ChartKind::Throughput, true, " eth0 · throughput ")]
    #[case::throughput_before_a_selection(
        ChartKind::Throughput,
        false,
        "waiting for the first sample"
    )]
    #[case::overview(ChartKind::Overview, false, " overview ")]
    #[case::totals(ChartKind::Totals, false, " session totals ")]
    #[case::health(ChartKind::Health, false, " errors & drops ")]
    fn draws_the_chosen_chart(
        #[case] kind: ChartKind,
        #[case] selected: bool,
        #[case] expected: &str,
    ) {
        let screen = draw(kind, selected);
        assert!(screen.contains(expected), "{screen}");
    }

    #[rstest]
    fn interfaces_missing_from_the_last_read_are_gone() {
        let mut history = History::new(10);
        assert_eq!(last_read(&history), None);
        history.record(0.0, stats("eth0", &[]));
        history.record(0.0, stats("veth0", &[]));
        history.record(1.0, stats("eth0", &[]));
        assert_eq!(last_read(&history), Some(1.0));
        let gone = |name| is_gone(history.get(name).expect("recorded"), last_read(&history));
        assert!(!gone("eth0"));
        assert!(gone("veth0"));
    }
}

#[cfg(test)]
mod test_support {
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    use crate::{
        model::{Counter, Counters, InterfaceStats},
        test_support::lines,
    };

    /// A parsed line for `name` with these counters set and the rest at zero.
    pub(super) fn stats<'a>(name: &'a str, values: &[(Counter, u64)]) -> InterfaceStats<'a> {
        let mut counters = Counters::default();
        for &(counter, value) in values {
            counters[counter] = value;
        }
        InterfaceStats { name, counters }
    }

    /// Renders `widget` into a `width` by `height` buffer and returns its rows as text.
    pub(super) fn render_lines(widget: impl Widget, width: u16, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        lines(&buf)
    }
}
