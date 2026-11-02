use clap::Parser;
use procnetmon::{
    app::App, cli::Cli, event_loop, history::History, logging, source::ProcNetMon, ui::UiState,
};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();
    logging::init(cli.log_file.as_deref())?;
    tracing::info!(
        file = %cli.file.display(),
        poll_interval_ms = cli.poll_interval_ms,
        window_secs = cli.window_secs,
        "starting"
    );

    let source = ProcNetMon::open(&cli.file)?;
    let history = History::new(cli.history_capacity());
    let app = App::new(history, UiState::default(), cli.window(), None);

    // ratatui::run restores the terminal however the loop ends: a quit, an error, or a panic.
    let result = ratatui::run(|terminal| {
        event_loop::run(
            terminal,
            app,
            source,
            cli.interval(),
            event_loop::crossterm_events,
        )
    });
    match &result {
        Ok(()) => tracing::info!("quit"),
        Err(err) => tracing::error!("{err}"),
    }
    result
}
