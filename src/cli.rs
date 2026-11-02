use clap::Parser;
use std::{path::PathBuf, time::Duration};

/// Live terminal charts for Linux network interface counters /proc/net/dev
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Time between samples, in milliseconds.
    #[arg(
        short = 'p',
        long = "poll-interval",
        value_name = "MS",
        default_value_t = 1000,
        value_parser = clap::value_parser!(u64).range(50..=60_000),
    )]
    pub poll_interval_ms: u64,

    /// How much history the charts show, in seconds.
    #[arg(
        short = 'w',
        long = "window",
        value_name = "SECS",
        default_value_t = 300,
        value_parser = clap::value_parser!(u64).range(10..=86_400)
    )]
    pub window_secs: u64,

    /// Counters file to read. Point it at a capture to develop on macOS.
    #[arg(
        short = 'f',
        long,
        value_name = "PATH",
        default_value = "/proc/net/dev"
    )]
    pub file: PathBuf,

    /// Write logs to PATH (off when omitted). Filter with RUST_LOG.
    #[expect(
        clippy::doc_markdown,
        reason = "clap prints this as --help text, where backticks would show"
    )]
    #[arg(short = 'l', long, value_name = "PATH")]
    pub log_file: Option<PathBuf>,
}

impl Cli {
    #[must_use]
    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }

    #[must_use]
    pub fn window(&self) -> Duration {
        Duration::from_secs(self.window_secs)
    }

    /// Samples kept per interface so the charts cover the whole window.
    #[must_use]
    pub fn history_capacity(&self) -> usize {
        let samples = self.window_secs * 1000 / self.poll_interval_ms + 1;
        usize::try_from(samples).unwrap_or(usize::MAX).max(2)
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;
    use rstest::rstest;

    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(["procnetmon"].iter().chain(args))
    }

    #[rstest]
    fn defaults_to_procnetmon_every_second_over_five_minutes() {
        let cli = parse(&[]).expect("every option has a default");
        assert_eq!(cli.interval(), Duration::from_secs(1));
        assert_eq!(cli.window(), Duration::from_secs(300));
        assert_eq!(cli.file, PathBuf::from("/proc/net/dev"));
        assert_eq!(cli.log_file, None);
    }

    #[rstest]
    #[case::short(&["-p", "250", "-w", "60", "-f", "capture.txt", "-l", "app.log"])]
    #[case::long(&[
        "--poll-interval", "250", "--window", "60", "--file", "capture.txt", "--log-file", "app.log"
    ])]
    fn reads_every_option(#[case] args: &[&str]) {
        let cli = parse(args).expect("valid options");
        assert_eq!(cli.interval(), Duration::from_millis(250));
        assert_eq!(cli.window(), Duration::from_secs(60));
        assert_eq!(cli.file, PathBuf::from("capture.txt"));
        assert_eq!(cli.log_file, Some(PathBuf::from("app.log")));
    }

    #[rstest]
    #[case::fastest_interval(&["-p", "50"])]
    #[case::slowest_interval(&["-p", "60000"])]
    #[case::shortest_window(&["-w", "10"])]
    #[case::longest_window(&["-w", "86400"])]
    fn accepts_the_limits(#[case] args: &[&str]) {
        assert!(parse(args).is_ok(), "{args:?}");
    }

    #[rstest]
    #[case::interval_too_short(&["-p", "49"])]
    #[case::interval_too_long(&["-p", "60001"])]
    #[case::window_too_short(&["-w", "9"])]
    #[case::window_too_long(&["-w", "86401"])]
    fn rejects_values_past_the_limits(#[case] args: &[&str]) {
        let err = parse(args).expect_err("out of range");
        assert_eq!(err.kind(), ErrorKind::ValueValidation, "{args:?}");
    }

    #[rstest]
    #[case::defaults(1_000, 300, 301)]
    #[case::fast_and_short(250, 60, 241)]
    #[case::uneven(1_500, 300, 201)]
    #[case::interval_longer_than_the_window(60_000, 10, 2)]
    fn keeps_enough_samples_to_cover_the_window(
        #[case] poll_interval_ms: u64,
        #[case] window_secs: u64,
        #[case] expected: usize,
    ) {
        let cli = Cli {
            poll_interval_ms,
            window_secs,
            file: PathBuf::new(),
            log_file: None,
        };
        assert_eq!(cli.history_capacity(), expected);
    }
}
