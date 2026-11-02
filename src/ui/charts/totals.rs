//! Bytes and packets each interface has moved since the app started.

use std::iter;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::Stylize,
    widgets::{Cell, Row, Table, Widget},
};

use crate::{
    history::{History, InterfaceHistory},
    model::Counter,
    ui::{
        charts::{NAME_WIDTH, is_gone, last_read, right_aligned, rows_block},
        format,
    },
};

/// What each interface moved this session. One that showed up later counts from then.
pub struct TotalsTable<'a> {
    history: &'a History,
}

impl<'a> TotalsTable<'a> {
    #[must_use]
    pub const fn new(history: &'a History) -> Self {
        Self { history }
    }
}

/// A column after the name: its header, the counter it totals, and how to print the total.
struct Column {
    title: &'static str,
    counter: Counter,
    show: fn(u64) -> String,
}

impl Column {
    const fn new(title: &'static str, counter: Counter, show: fn(u64) -> String) -> Self {
        Self {
            title,
            counter,
            show,
        }
    }

    fn total(&self, history: &InterfaceHistory) -> String {
        (self.show)(history.session_total(self.counter))
    }
}

const COLUMNS: [Column; 4] = [
    Column::new("rx bytes", Counter::RxBytes, format::bytes),
    Column::new("tx bytes", Counter::TxBytes, format::bytes),
    Column::new("rx packets", Counter::RxPackets, format::count),
    Column::new("tx packets", Counter::TxPackets, format::count),
];

/// Fits the widest totals, like "1023.9 KiB", and the "rx packets" header.
const TOTAL_WIDTH: u16 = 10;

impl Widget for TotalsTable<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header = Row::new(
            iter::once(Cell::from("interface"))
                .chain(COLUMNS.iter().map(|column| right_aligned(column.title))),
        )
        .bold();
        let widths = iter::once(Constraint::Length(NAME_WIDTH))
            .chain(COLUMNS.iter().map(|_| Constraint::Length(TOTAL_WIDTH)));
        let last_read = last_read(self.history);
        let rows = self
            .history
            .iter()
            .map(|(name, history)| row(name, history, is_gone(history, last_read)));
        Table::new(rows, widths)
            .header(header)
            .block(rows_block(
                "session totals",
                self.history.iter().count(),
                area,
            ))
            .render(area, buf);
    }
}

/// An interface's totals. One that's gone still counts toward the session, but dimmed.
fn row<'a>(name: &'a str, history: &InterfaceHistory, gone: bool) -> Row<'a> {
    let totals = COLUMNS
        .iter()
        .map(|column| right_aligned(column.total(history)));
    let row = Row::new(iter::once(Cell::from(name)).chain(totals));
    if gone { row.dark_gray() } else { row }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;
    use rstest::rstest;

    use super::*;
    use crate::ui::charts::test_support::{render_lines, stats};

    #[rstest]
    fn renders_what_each_interface_moved() {
        let mut history = History::new(10);
        history.record(0.0, stats("eth0", &[]));
        let moved = [
            (Counter::RxBytes, 1536),
            (Counter::TxBytes, 3 << 20),
            (Counter::RxPackets, 12_345),
            (Counter::TxPackets, 42),
        ];
        history.record(1.0, stats("eth0", &moved));
        let lines = render_lines(TotalsTable::new(&history), 70, 4);
        assert!(lines[1].contains("rx packets"), "{lines:#?}");
        for total in ["│eth0", "1.5 KiB", "3.0 MiB", "12.3 k", "42"] {
            assert!(lines[2].contains(total), "{total} is missing: {lines:#?}");
        }
    }

    #[rstest]
    fn a_vanished_interface_is_dimmed() {
        let mut history = History::new(10);
        history.record(0.0, stats("eth0", &[]));
        history.record(0.0, stats("veth0", &[]));
        history.record(1.0, stats("eth0", &[]));
        let area = Rect::new(0, 0, 70, 5);
        let mut buf = Buffer::empty(area);
        TotalsTable::new(&history).render(area, &mut buf);
        // The first letter of each name, below the border and the header.
        assert_eq!(buf[(1, 2)].symbol(), "e");
        assert_eq!(buf[(1, 2)].fg, Color::Reset);
        assert_eq!(buf[(1, 3)].symbol(), "v");
        assert_eq!(buf[(1, 3)].fg, Color::DarkGray);
    }

    #[rstest]
    fn says_how_many_interfaces_did_not_fit() {
        let mut history = History::new(10);
        for name in ["lo", "eth0", "wlan0"] {
            history.record(0.0, stats(name, &[]));
        }
        // Room for the header and one row.
        let lines = render_lines(TotalsTable::new(&history), 70, 4);
        assert!(lines[2].starts_with("│lo"), "{lines:#?}");
        assert!(lines[3].contains("+2 more"), "{lines:#?}");
    }
}
