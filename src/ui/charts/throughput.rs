use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Stylize},
    symbols::Marker,
    text::Line,
    widgets::{Axis, Block, Chart, Dataset, GraphType, Widget},
};

use crate::history::InterfaceHistory;
use crate::model::Counter;
use crate::ui::format;

/// rx/tx bytes per second for one interface over a sliding window.
pub struct ThroughputChart<'a> {
    pub name: &'a str,
    pub history: &'a InterfaceHistory,
    pub window_secs: f64,
}

impl Widget for ThroughputChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Bounded by the ring buffer (about window/interval points), so collecting per frame is cheap.
        let rx: Vec<(f64, f64)> = self.history.rates(Counter::RxBytes).collect();
        let tx: Vec<(f64, f64)> = self.history.rates(Counter::TxBytes).collect();

        let now = self.history.latest().map_or(0.0, |s| s.t);
        let y_max = rx.iter().chain(&tx).map(|&(_, y)| y).fold(1.0, f64::max) * 1.1;

        Chart::new(vec![
            line("rx", Color::Cyan, &rx),
            line("tx", Color::Magenta, &tx),
        ])
        .block(Block::bordered().title(Line::from(format!(" {} · throughput ", self.name)).bold()))
        .x_axis(
            Axis::default()
                .bounds([now - self.window_secs, now])
                .labels([format!("-{:.0}s", self.window_secs), "now".to_owned()]),
        )
        .y_axis(Axis::default().bounds([0.0, y_max]).labels([
            "0".to_owned(),
            format::rate(y_max / 2.0),
            format::rate(y_max),
        ]))
        .render(area, buf);
    }
}

fn line<'a>(name: &'a str, color: Color, data: &'a [(f64, f64)]) -> Dataset<'a> {
    Dataset::default()
        .name(name)
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(color)
        .data(data)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{
        history::History,
        ui::charts::test_support::{render_lines, stats},
    };

    /// `secs` seconds of eth0 receiving `rx` bytes per second and sending half that.
    fn eth0(rx: u64, secs: u32) -> History {
        let mut history = History::new(400);
        for t in 0..=secs {
            let received = u64::from(t) * rx;
            let counters = [
                (Counter::RxBytes, received),
                (Counter::TxBytes, received / 2),
            ];
            history.record(f64::from(t), stats("eth0", &counters));
        }
        history
    }

    fn render(history: &History, window_secs: f64) -> Vec<String> {
        let eth0 = history.get("eth0").expect("eth0 was recorded");
        let chart = ThroughputChart {
            name: "eth0",
            history: eth0,
            window_secs,
        };
        render_lines(chart, 60, 12)
    }

    /// The columns that have a plotted point in them.
    fn plotted_columns(lines: &[String]) -> Vec<usize> {
        let mut columns: Vec<usize> = lines
            .iter()
            .flat_map(|line| {
                line.chars()
                    .enumerate()
                    .filter(|&(_, c)| ('\u{2801}'..='\u{28ff}').contains(&c))
                    .map(|(column, _)| column)
            })
            .collect();
        columns.sort_unstable();
        columns.dedup();
        columns
    }

    #[rstest]
    fn labels_the_axes_from_the_data() {
        // A 2,000 B/s peak, so the y axis tops out 10% higher, at 2,200 B/s.
        let lines = render(&eth0(2_000, 10), 300.0);
        let screen = lines.join("\n");
        assert!(lines[0].contains(" eth0 · throughput "), "{screen}");
        assert!(screen.contains("2.1 KiB/s"), "the top: {screen}");
        assert!(screen.contains("1.1 KiB/s"), "the middle: {screen}");
        assert!(
            screen.contains("-300s") && screen.contains("now"),
            "{screen}"
        );
    }

    #[rstest]
    fn plots_recent_points_at_the_right_edge_of_the_window() {
        // Ten seconds of data in a 300-second window only reach the last few columns.
        let lines = render(&eth0(2_000, 10), 300.0);
        let columns = plotted_columns(&lines);
        assert!(!columns.is_empty(), "nothing plotted: {lines:#?}");
        assert!(columns.iter().all(|&column| column > 50), "{lines:#?}");
    }

    #[rstest]
    fn a_full_window_spans_the_chart() {
        let lines = render(&eth0(2_000, 300), 300.0);
        let columns = plotted_columns(&lines);
        assert!(
            columns.first().is_some_and(|&first| first < 20),
            "{lines:#?}"
        );
    }
}
