//! Optional logging to a file. The TUI owns the terminal, so logs go to a file or nowhere.

use std::{
    fs::OpenOptions,
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use tracing::{Subscriber, level_filters::LevelFilter, subscriber::SetGlobalDefaultError};
use tracing_subscriber::{EnvFilter, fmt::MakeWriter};

/// Starts appending logs to `path`, filtered by `RUST_LOG` and at `info` unless it says otherwise.
/// Without a path nothing is logged at all.
///
/// # Errors
///
/// If the file can't be opened, or logging was already started.
pub fn init(path: Option<&Path>) -> Result<(), LoggingError> {
    let Some(path) = path else {
        return Ok(());
    };
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| LoggingError::Open {
            path: path.to_owned(),
            source,
        })?;
    tracing::subscriber::set_global_default(subscriber(Mutex::new(file), env_filter()))?;
    Ok(())
}

/// `RUST_LOG`'s directives, at `info` by default. A directive that doesn't parse is skipped with a
/// warning on stderr, before the TUI takes over the terminal, rather than stopping the app.
fn env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy()
}

/// Plain-text log lines, without the color codes a terminal would want.
pub(crate) fn subscriber<W>(writer: W, filter: EnvFilter) -> impl Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .finish()
}

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("could not open log file {}", .path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("logging was already started")]
    AlreadyStarted(#[from] SetGlobalDefaultError),
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_support::Logs;

    #[rstest]
    fn logs_plain_lines_at_info_and_above() {
        let logs = Logs::capture("info", || {
            tracing::debug!("too detailed");
            tracing::info!("worth knowing");
            tracing::warn!("worth a look");
        });
        assert!(!logs.contains("too detailed"), "{logs}");
        assert!(
            logs.contains("INFO") && logs.contains("worth knowing"),
            "{logs}"
        );
        assert!(
            logs.contains("WARN") && logs.contains("worth a look"),
            "{logs}"
        );
        assert!(
            !logs.contains('\x1b'),
            "a file gets no color codes: {logs:?}"
        );
    }

    #[rstest]
    fn a_filter_can_turn_on_debug() {
        let logs = Logs::capture("debug", || tracing::debug!("too detailed"));
        assert!(logs.contains("too detailed"), "{logs}");
    }

    #[rstest]
    fn without_a_path_there_is_nothing_to_set_up() {
        init(None).expect("nothing can fail");
    }

    // The only test that sets the process-wide subscriber, which can only happen once. Other
    // tests capture logs with a subscriber scoped to their own thread, so they aren't affected.
    #[rstest]
    fn logs_to_the_file_and_starts_only_once() {
        let folder = std::env::temp_dir();
        let path = folder.join(format!("procnetmon-{}-init.log", std::process::id()));
        init(Some(&path)).expect("the first start works");
        tracing::error!("written to the log file");
        let logged = std::fs::read_to_string(&path).expect("init created the file");
        let again = init(Some(&path));
        let _ = std::fs::remove_file(&path);
        assert!(logged.contains("written to the log file"), "{logged:?}");
        assert!(
            matches!(again, Err(LoggingError::AlreadyStarted(_))),
            "{again:?}"
        );
    }

    #[rstest]
    fn an_unopenable_file_says_which() {
        let err = init(Some(Path::new("/no/such/folder/procnetmon.log")))
            .expect_err("the folder doesn't exist");
        assert_eq!(
            err.to_string(),
            "could not open log file /no/such/folder/procnetmon.log"
        );
        assert!(std::error::Error::source(&err).is_some());
    }
}
