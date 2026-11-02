//! The app's one loop: sample on schedule, redraw, then handle the next key.

use std::{
    io,
    time::{Duration, Instant},
};

use ratatui::{
    Terminal,
    backend::Backend,
    crossterm::event::{self, Event},
};

use crate::{
    app::{App, Flow},
    keymap,
    source::ProcNetMon,
    ticker::Ticker,
    ui,
};

/// Runs until a key asks to quit.
///
/// Each pass samples `source` if a tick is due, redraws, and then waits for one event from
/// `next_event` for at most the time left until the next tick. The app passes
/// [`crossterm_events`]; tests pass a script, and a [`TestBackend`](ratatui::backend::TestBackend).
///
/// # Errors
///
/// If drawing fails or `next_event` does, for example because the terminal went away.
pub fn run<B>(
    terminal: &mut Terminal<B>,
    mut app: App,
    mut source: ProcNetMon,
    interval: Duration,
    mut next_event: impl FnMut(Duration) -> io::Result<Option<Event>>,
) -> color_eyre::Result<()>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let start = Instant::now();
    let mut ticker = Ticker::new(interval, start);
    loop {
        let now = Instant::now();
        if ticker.poll(now) {
            app.on_sample(now.duration_since(start).as_secs_f64(), source.read());
        }
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        if let Some(key) = next_event(ticker.timeout(Instant::now()))?
            .as_ref()
            .and_then(Event::as_key_press_event)
            && let Some(action) = keymap::action_for(key)
            && app.update(action) == Flow::Quit
        {
            return Ok(());
        }
    }
}

/// Waits up to `timeout` for the terminal's next event.
///
/// # Errors
///
/// If the terminal can't be read.
#[mutants::skip] // It needs a real terminal, so the loop's tests pass a scripted event source.
pub fn crossterm_events(timeout: Duration) -> io::Result<Option<Event>> {
    if event::poll(timeout)? {
        event::read().map(Some)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use ratatui::{
        backend::TestBackend,
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    };
    use rstest::rstest;

    use super::*;
    use crate::{
        history::History,
        test_support::{CAPTURE, TempFile, lines},
        ui::UiState,
    };

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// Runs the loop on a test terminal over a capture, feeding it `script` one event per wait,
    /// where `None` is a wait that timed out. Returns the result, the screen, and the events left.
    fn run_script(
        name: &str,
        script: Vec<Option<Event>>,
    ) -> (color_eyre::Result<()>, Vec<String>, usize) {
        let counters = TempFile::new(name, CAPTURE);
        let source = ProcNetMon::open(counters.path()).expect("the capture opens");
        let app = App::new(
            History::new(10),
            UiState::default(),
            Duration::from_secs(300),
            None,
        );
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).expect("a test terminal");
        let mut script = VecDeque::from(script);
        let result = run(&mut terminal, app, source, Duration::from_secs(1), |_| {
            script
                .pop_front()
                .ok_or_else(|| io::Error::other("the script ran out"))
        });
        (result, lines(terminal.backend().buffer()), script.len())
    }

    #[rstest]
    fn samples_draws_and_quits_on_q() {
        let (result, screen, left) =
            run_script("loop-quit", vec![None, Some(key(KeyCode::Char('q')))]);
        result.expect("q ends the loop cleanly");
        assert_eq!(left, 0);
        let screen = screen.join("\n");
        assert!(screen.contains("eth0"), "the capture was sampled: {screen}");
        assert!(screen.contains("q/Esc quit"), "{screen}");
    }

    #[rstest]
    fn keeps_going_after_other_keys() {
        let script = [KeyCode::Down, KeyCode::Enter, KeyCode::Esc]
            .map(|code| Some(key(code)))
            .to_vec();
        let (result, screen, left) = run_script("loop-keys", script);
        result.expect("Esc ends the loop cleanly");
        assert_eq!(left, 0, "every key was handled before quitting");
        let screen = screen.join("\n");
        assert!(
            screen.contains("> ifb0"),
            "Down selected the second interface: {screen}"
        );
    }

    #[rstest]
    fn a_failing_event_source_ends_the_loop_with_its_error() {
        let (result, _, _) = run_script("loop-error", vec![None]);
        let err = result.expect_err("the script ran out");
        assert!(err.to_string().contains("the script ran out"), "{err}");
    }
}
