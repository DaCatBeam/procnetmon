pub mod charts;
pub mod format;
pub mod legend;
use charts::ChartKind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, List, ListState},
};

use crate::{
    app::App,
    ui::{
        charts::{ViewContext, render_chart},
        legend::legend,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Interfaces,
    Charts,
}

impl Focus {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Interfaces => Self::Charts,
            Self::Charts => Self::Interfaces,
        }
    }

    #[must_use]
    pub const fn prev(self) -> Self {
        self.next()
    }
}

#[derive(Debug)]
pub struct UiState {
    pub focus: Focus,
    pub interfaces: ListState,
    pub charts: ListState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            focus: Focus::default(),
            interfaces: ListState::default().with_selected(Some(0)),
            charts: ListState::default().with_selected(Some(0)),
        }
    }
}

impl UiState {
    pub fn focused_list(&mut self) -> &mut ListState {
        match self.focus {
            Focus::Interfaces => &mut self.interfaces,
            Focus::Charts => &mut self.charts,
        }
    }

    #[must_use]
    pub fn selected_chart(&self) -> ChartKind {
        self.charts
            .selected()
            .and_then(|i| ChartKind::ALL.get(i).copied())
            .unwrap_or_default()
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let [_header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [sidebar, main] =
        Layout::horizontal([Constraint::Length(24), Constraint::Fill(1)]).areas(body);
    let charts_height = u16::try_from(ChartKind::ALL.len() + 2).unwrap_or(u16::MAX);
    let [iface_area, chart_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(charts_height)]).areas(sidebar);

    let focus = app.ui.focus;
    render_list(
        frame,
        iface_area,
        " interfaces ",
        app.history.names(),
        &mut app.ui.interfaces,
        focus == Focus::Interfaces,
    );
    render_list(
        frame,
        chart_area,
        " charts ",
        ChartKind::ALL.map(ChartKind::title),
        &mut app.ui.charts,
        focus == Focus::Charts,
    );

    let ctx = ViewContext {
        history: &app.history,
        selected: app
            .ui
            .interfaces
            .selected()
            .and_then(|i| app.history.iter().nth(i)),
        window: app.window,
    };
    render_chart(app.ui.selected_chart(), &ctx, frame, main);
    frame.render_widget(legend(footer.width), footer);
}

fn render_list<'a>(
    frame: &mut Frame,
    area: Rect,
    title: &'a str,
    items: impl IntoIterator<Item = &'a str>,
    state: &mut ListState,
    focused: bool,
) {
    let border = if focused {
        Style::new().cyan()
    } else {
        Style::new().dark_gray()
    };
    let list = List::new(items) // &str items: no clone of the interface names
        .block(Block::bordered().title(title).border_style(border))
        .highlight_style(Modifier::REVERSED)
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, state);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};
    use rstest::rstest;

    use super::*;
    use crate::{
        app::Action,
        history::History,
        test_support::{CAPTURE, lines},
    };

    fn app() -> App {
        let mut app = App::new(
            History::new(10),
            UiState::default(),
            Duration::from_secs(300),
            None,
        );
        app.on_sample(0.0, Ok(CAPTURE));
        app
    }

    /// Draws the whole UI on a `width` by `height` test terminal.
    fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
        terminal.draw(|frame| render(frame, app)).expect("a frame");
        terminal.backend().buffer().clone()
    }

    #[rstest]
    fn focus_alternates_between_the_two_lists() {
        assert_eq!(Focus::Interfaces.next(), Focus::Charts);
        assert_eq!(Focus::Charts.next(), Focus::Interfaces);
        assert_eq!(Focus::Interfaces.prev(), Focus::Charts);
        assert_eq!(Focus::Charts.prev(), Focus::Interfaces);
    }

    #[rstest]
    fn keys_move_only_the_focused_list() {
        let mut ui = UiState::default();
        ui.focused_list().select(Some(3));
        assert_eq!(ui.interfaces.selected(), Some(3));
        ui.focus = Focus::Charts;
        ui.focused_list().select(Some(2));
        assert_eq!(ui.charts.selected(), Some(2));
        assert_eq!(ui.interfaces.selected(), Some(3));
    }

    #[rstest]
    #[case::first(Some(0), ChartKind::Throughput)]
    #[case::third(Some(2), ChartKind::Totals)]
    #[case::last(Some(3), ChartKind::Health)]
    #[case::past_the_end(Some(9), ChartKind::Throughput)]
    #[case::none(None, ChartKind::Throughput)]
    fn the_chart_list_picks_the_chart(
        #[case] selected: Option<usize>,
        #[case] expected: ChartKind,
    ) {
        let mut ui = UiState::default();
        ui.charts.select(selected);
        assert_eq!(ui.selected_chart(), expected);
    }

    #[rstest]
    fn the_sidebar_fits_both_lists_and_shows_which_has_focus() {
        let mut app = app();
        // 12 rows leave the interface list two, above the four charts.
        let buffer = draw(&mut app, 100, 12);
        let sidebar: Vec<String> = lines(&buffer)
            .iter()
            .map(|line| line.chars().take(24).collect())
            .collect();
        let sidebar = sidebar.join("\n");
        assert!(sidebar.contains("> lo"), "{sidebar}");
        assert!(sidebar.contains("errors & drops"), "{sidebar}");
        // The focused list's border is cyan and the other's dark gray.
        let corners = |buffer: &Buffer| (buffer[(0, 1)].fg, buffer[(0, 5)].fg);
        assert_eq!(corners(&buffer), (Color::Cyan, Color::DarkGray));

        app.update(Action::FocusNext);
        assert_eq!(
            corners(&draw(&mut app, 100, 12)),
            (Color::DarkGray, Color::Cyan)
        );
    }

    #[rstest]
    fn the_main_area_shows_the_chosen_chart() {
        let mut app = app();
        app.ui.charts.select(Some(2));
        let screen = lines(&draw(&mut app, 100, 12)).join("\n");
        assert!(screen.contains(" session totals "), "{screen}");
    }

    #[rstest]
    fn the_footer_shows_the_key_legend() {
        let footer = lines(&draw(&mut app(), 100, 12)).remove(11);
        let expected: String = legend(100)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(expected.contains("quit"), "{expected:?}");
        assert_eq!(footer.trim_end(), expected);
    }
}
