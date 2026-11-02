//! The app's state, and what keys and fresh samples do to it.

use std::{error::Error, time::Duration};

use crate::{history::History, parse::parse, source::SourceError, ui::UiState};

/// Everything the UI draws from.
pub struct App {
    pub history: History,
    pub ui: UiState,
    pub window: Duration,
    /// The problem with the latest read, if it had one.
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    FocusNext,
    FocusPrev,
    SelectNext,
    SelectPrev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

impl App {
    #[must_use]
    pub fn new(history: History, ui: UiState, window: Duration, status: Option<String>) -> Self {
        Self {
            history,
            ui,
            window,
            status,
        }
    }

    pub fn update(&mut self, action: Action) -> Flow {
        match action {
            Action::Quit => return Flow::Quit,
            Action::FocusNext => self.ui.focus = self.ui.focus.next(),
            Action::FocusPrev => self.ui.focus = self.ui.focus.prev(),
            Action::SelectNext => self.ui.focused_list().select_next(),
            Action::SelectPrev => self.ui.focused_list().select_previous(),
        }
        Flow::Continue
    }

    /// Records one read of the counters file, taken `t` seconds after the app started.
    ///
    /// A problem with the read becomes the `status`, and a clean read clears it. Changes are
    /// logged, so a file that stays missing is logged once rather than on every tick.
    pub fn on_sample(&mut self, t: f64, read: Result<&str, SourceError>) {
        let mut problem = None;
        match read {
            Ok(contents) => {
                for line in parse(contents) {
                    match line {
                        Ok(stats) => self.history.record(t, stats),
                        Err(err) => problem = Some(describe(&err)),
                    }
                }
            }
            Err(err) => problem = Some(describe(&err)),
        }
        if problem != self.status {
            if let Some(message) = &problem {
                tracing::warn!("{message}");
            } else {
                tracing::info!("reading cleanly again");
            }
            self.status = problem;
        }
    }
}

/// An error and its causes on one line, like "could not read /proc/net/dev: permission denied".
fn describe(err: &dyn Error) -> String {
    let mut text = err.to_string();
    let mut cause = err.source();
    while let Some(source) = cause {
        text.push_str(": ");
        text.push_str(&source.to_string());
        cause = source.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use std::{io, path::PathBuf};

    use rstest::rstest;

    use super::*;
    use crate::{
        test_support::{CAPTURE, Logs},
        ui::Focus,
    };

    fn app() -> App {
        App::new(
            History::new(10),
            UiState::default(),
            Duration::from_secs(300),
            None,
        )
    }

    fn unreadable() -> SourceError {
        SourceError::Read {
            path: PathBuf::from("/proc/net/dev"),
            source: io::Error::other("disk on fire"),
        }
    }

    #[rstest]
    fn records_every_interface_in_a_read() {
        let mut app = app();
        app.on_sample(0.0, Ok(CAPTURE));
        let names: Vec<_> = app.history.names().collect();
        assert_eq!(names, ["lo", "ifb0", "ifb1", "eth0"]);
        assert_eq!(app.status, None);
    }

    #[rstest]
    fn shows_the_latest_problem_with_its_cause_until_a_clean_read() {
        let mut app = app();
        app.on_sample(0.0, Err(unreadable()));
        assert_eq!(
            app.status.as_deref(),
            Some("could not read /proc/net/dev: disk on fire")
        );
        app.on_sample(1.0, Ok("not the counters file"));
        assert_eq!(
            app.status.as_deref(),
            Some("line 1: not a /proc/net/dev header")
        );
        app.on_sample(2.0, Ok(CAPTURE));
        assert_eq!(app.status, None);
    }

    #[rstest]
    fn logs_a_problem_once_and_then_the_recovery() {
        let mut app = app();
        let logs = Logs::capture("info", || {
            app.on_sample(0.0, Ok(CAPTURE));
            app.on_sample(1.0, Err(unreadable()));
            app.on_sample(2.0, Err(unreadable()));
            app.on_sample(3.0, Ok(CAPTURE));
        });
        assert_eq!(logs.matches("disk on fire").count(), 1, "{logs}");
        assert!(logs.contains("WARN"), "{logs}");
        assert_eq!(logs.matches("reading cleanly again").count(), 1, "{logs}");
    }

    #[rstest]
    fn only_quit_stops_the_loop() {
        let mut app = app();
        for action in [
            Action::FocusNext,
            Action::FocusPrev,
            Action::SelectNext,
            Action::SelectPrev,
        ] {
            assert_eq!(app.update(action), Flow::Continue, "{action:?}");
        }
        assert_eq!(app.update(Action::Quit), Flow::Quit);
    }

    #[rstest]
    fn focus_moves_between_the_two_lists() {
        let mut app = app();
        assert_eq!(app.ui.focus, Focus::Interfaces);
        app.update(Action::FocusNext);
        assert_eq!(app.ui.focus, Focus::Charts);
        app.update(Action::FocusPrev);
        assert_eq!(app.ui.focus, Focus::Interfaces);
    }

    #[rstest]
    fn selection_moves_in_the_focused_list_only() {
        let mut app = app();
        app.update(Action::SelectNext);
        assert_eq!(app.ui.interfaces.selected(), Some(1));
        assert_eq!(app.ui.charts.selected(), Some(0));

        app.update(Action::FocusNext);
        app.update(Action::SelectNext);
        app.update(Action::SelectNext);
        app.update(Action::SelectPrev);
        assert_eq!(app.ui.charts.selected(), Some(1));
        assert_eq!(app.ui.interfaces.selected(), Some(1));
    }
}
