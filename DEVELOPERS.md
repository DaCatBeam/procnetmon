# Developing proc-net-dev

How the code is laid out, how data moves through it, and how to build, run, debug, and test it.

## Project structure

```text
.
├── .github/
│   └── workflows/ci.yml  # CI: formatting, clippy and tests on every PR and on main
├── Cargo.toml            # the crate: one binary, plus a library the binary and tests share
├── Makefile              # run-dev, run-tests, debug-tests, run-mutants, run-fmt, record-demo
├── Dockerfile            # Linux image that make run-dev builds and runs
├── AGENTS.md             # instructions for AI agents (CLAUDE.md imports it)
├── README.md             # for users
├── DEVELOPERS.md         # this file
├── ROADMAP.md            # planned work
├── LICENSE               # MIT
├── docs/
│   ├── demo.gif          # the recording for the README
│   └── demo.tape         # records docs/demo.gif with VHS (make record-demo)
├── examples/
│   └── demo_traffic.rs   # simulated traffic for the demo, as a fake /proc/net/dev
├── src/
│   ├── main.rs           # entrypoint: error reports, options, logging, terminal setup
│   ├── lib.rs            # module list
│   ├── cli.rs            # command-line options (clap)
│   ├── logging.rs        # optional logging to a file (tracing)
│   ├── source.rs         # ProcNetDev: one open handle to the counters file, re-read each tick
│   ├── parse.rs          # parse(&str): a lazy iterator of InterfaceStats or ParseError
│   ├── model.rs          # Counter, Counters, InterfaceStats, Unit
│   ├── history.rs        # a bounded window of samples per interface, rates, session totals
│   ├── ticker.rs         # a sampling schedule that doesn't drift
│   ├── app.rs            # App state, and what keys and samples do to it
│   ├── keymap.rs         # key bindings, which also feed the footer's legend
│   ├── event_loop.rs     # the loop: sample, draw, wait for a key
│   ├── test_support.rs   # helpers shared by the unit tests
│   └── ui/
│       ├── mod.rs        # layout, the two sidebar panes, focus
│       ├── legend.rs     # the footer's key legend
│       ├── format.rs     # readable rates, byte sizes, and counts
│       └── charts/       # one module per chart: throughput, overview, totals, health
└── tests/
    ├── cli.rs            # runs the built binary
    ├── parse_captures.rs # parses the real captures in fixtures/
    ├── procfs_live.rs    # reads this machine's /proc/net/dev (Linux only)
    └── fixtures/         # real /proc/net/dev captures, kept byte for byte
```

## Data flows

Everything runs on one thread, in `event_loop::run`. Each pass through the loop does three things:

1. **Sample, if one is due.** The `Ticker` decides. `ProcNetDev::read` rewinds the open counters file and reads it into a reused `String`. `App::on_sample` parses that with `parse::parse`, a lazy iterator whose `InterfaceStats` borrow their names from the buffer, and `History::record` adds each interface's counters to its window. A failed read or a line that doesn't parse becomes `App.status` and is logged when it changes. Lines that parse are recorded either way.
2. **Draw.** `ui::render` draws the whole screen from `App`: the two sidebar panes, the selected chart, and the key legend. Charts read `History` through a `ViewContext`.
3. **Wait for input.** The loop waits for one terminal event, for at most the time left until the next sample. A key press maps to an `Action` through `keymap::action_for`, and `App::update` applies it. `Action::Quit` ends the loop.

```text
                   +------------------------- one thread: event_loop::run -------------------------+
 /proc/net/dev --> | ProcNetDev::read() --> App::on_sample() --> parse(&str) --> History::record() |
  (or --file)      | rewinds one handle     problems become      lazy; borrows   a window of       |
                   | into a reused String   App.status, logged   names from the  samples per       |
                   |                        once per change      buffer          interface, plus   |
                   |                                                             session totals    |
                   |                                                                               |
 keyboard -------> | next_event(ticker.timeout()) --> keymap::action_for() --> App::update(Action) |
 (Ctrl+C is a key) | crossterm_events in the app,                              Quit ends the loop  |
                   | a script in tests                                                             |
                   |                                                                               |
                   | ui::render(frame, &mut App)                                                   |
                   |   +- sidebar: interface list (first-seen order) and chart list                |
                   |   +- main:    the selected chart, drawn from History                          |
                   |   +- footer:  key legend built from keymap::BINDINGS                          |
                   +-------------------------------------------------------------------------------+
  ratatui::run wraps the loop: raw mode and the alternate screen on entry, restored on return or panic.
  With --log-file, each log line is written to the file from this same thread.
```

A sample's time is the seconds since startup, measured with `Instant`, so rates don't jump when the wall clock changes.

### Design choices

- **One loop, no channels.** A sample takes microseconds, far less than a frame, so nothing needs another thread. Quitting is a `return`, and `ratatui::run` restores the terminal on every exit path except a signal. If a data source could ever block, it would move to its own thread and send its samples to the loop.
- **Events come in as a function.** `event_loop::run` takes the terminal and the event source as parameters. The app passes a real terminal and `crossterm_events`, and tests pass a `TestBackend` and a script.
- **Typed counters.** A `Counter` enum indexes a `[u64; 16]` array, so a column can't be read as the wrong counter. A compile-time check keeps `Counter::ALL` in declaration order, and a test checks that order against the kernel's header. Each counter also has a `Unit`, so a chart can tell which counters may share an axis. The current charts pick their counters directly and don't use it yet.
- **Rates from `u64` deltas.** Counters are subtracted as `u64` first, and a decrease is treated as a reset. Only the small per-interval delta is converted to `f64` for plotting, so precision holds even for counters past 2^53.
- **Bounded history.** Each interface keeps `window / interval + 1` samples of 136 bytes each (`Cli::history_capacity`), so memory stays flat however long the app runs. Session totals are tracked separately and aren't limited by the window. Interfaces are never removed, so a selected index keeps pointing at the same interface.
- **Errors live where they happen.** `SourceError` (I/O), `ParseError` (malformed lines), and `LoggingError` are `thiserror` enums next to the code that produces them. `main` reports startup failures with `color-eyre` and exits non-zero. While running, a problem goes to `App.status` and the log instead of stopping the app. Nothing draws `App.status` yet; see [ROADMAP.md](ROADMAP.md).
- **Deliberately left out:** async (there are no network sources), chart plugins via trait objects (an enum covers a fixed set of charts), and downsampling (there's no zoom yet).

## Entrypoints

- `src/main.rs` is the `proc-net-dev` binary. It installs `color-eyre`'s error and panic reports, parses the options, starts logging, opens the counters file, sizes the history from the options, and then runs `event_loop::run` inside `ratatui::run`.
- `src/lib.rs` is the `proc_net_dev` library that the binary and the integration tests use.
- Each `.rs` file in `tests/` is its own test binary, run by `cargo test`.

## Runbooks and skills

- `AGENTS.md` holds the instructions for AI agents, including the `AGENT_QUESTION:` and `AGENT_INSTRUCTION:` comment conventions. `CLAUDE.md` imports it for Claude Code.
- README.md and this file follow the `project-readme` and `project-developers-doc` agent skills. Use them to update both after changing setup, options, keys, tooling, or the project's layout.

## Setup after cloning

```sh
# Rust, if you don't have it yet: https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Tools for mutation testing and for finding unused dependencies
cargo install cargo-binstall
cargo binstall cargo-mutants
cargo install cargo-machete

# Build everything and run the tests
make run-tests
```

On Linux, `cargo run` starts the app on the machine's own `/proc/net/dev`. macOS has no `/proc/net/dev`, so there are two ways to run it there:

- **Natively, from a capture.** `cargo run -- --file tests/fixtures/linux-6.18.txt` runs on the test fixture, but it never changes, so every rate is zero. For moving charts, run `cat /proc/net/dev > capture.txt` on a Linux machine, copy the file over, and run `cargo run -- --file capture.txt`. The app re-reads the file on every tick, so overwriting it in place with a newer capture (`cat newer.txt > capture.txt`) moves the charts. Replacing the file instead, with `mv` or an editor that saves a new copy, leaves the app reading the old one, because it keeps the file open.
- **In Docker.** `make run-dev` builds a `linux/amd64` image from the Dockerfile, rebuilding from scratch each time, and runs a debug build of the app in a container called `proc-net-dev`. It shows the container's own interfaces, usually `lo` and `eth0`.

## Default behaviors

With no options, the app:

- Reads `/proc/net/dev` once at startup and then every 1000 ms, on a schedule that doesn't drift or burst to catch up after a stall.
- Keeps 301 samples per interface, the last 5 minutes, which is about 40 KB per interface.
- Opens on the throughput chart for the first interface in the file, usually `lo`, with the interface pane focused.
- Logs nothing.
- Exits with status 1 and an error report, before taking over the terminal, if the counters file can't be opened. Invalid options exit with status 2.
- Keeps running when a read fails or a line doesn't parse. The lines that parse are still recorded, and the problem is kept in `App.status` and logged if `--log-file` is set.

## Logging and debugging

The TUI owns the terminal, so logs go to a file or nowhere. `--log-file PATH` appends plain-text lines to `PATH`, written straight from the app's one thread. `RUST_LOG` filters them with [tracing directives](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives), at `info` by default. A directive that doesn't parse is skipped with a warning on stderr.

At the default level, the log has a `starting` line with the options, a warning for each new read or parse problem, a line when reads are clean again, and a `quit` line or the error that ended the app. Follow it from a second terminal:

```sh
cargo run -- --log-file /tmp/proc-net-dev.log    # terminal 1
tail -f /tmp/proc-net-dev.log                    # terminal 2
```

Errors that stop the app and panics are printed by `color-eyre` after the terminal is restored. Set `RUST_BACKTRACE=1` to include a backtrace.

A debugger can't share the terminal with the TUI, so start the app in one terminal and attach from another. Attaching may ask for your password, or need `sudo` on Linux.

```sh
rust-lldb -p "$(pgrep -n proc-net-dev)"    # macOS
rust-gdb -p "$(pgrep -n proc-net-dev)"     # Linux
```

For the loop's own logic, a test is usually quicker: the `event_loop` tests run it without a terminal, and `make debug-tests` shows test output.

## Linting and formatting

```sh
make run-fmt                                         # cargo fmt
cargo clippy --all-targets -- -W clippy::pedantic    # no make target yet
cargo machete                                        # lists unused dependencies
```

Clippy runs with the pedantic lints on, and the code has no warnings. Where a lint has to be silenced, use `#[expect(lint, reason = "...")]`, like the existing exceptions in `cli.rs`, `history.rs`, and `overview.rs`, so the reason is written down and the attribute goes stale loudly if the lint stops firing.

CI runs `cargo fmt --check` and the clippy command on every pull request, with clippy's warnings made errors (`-D warnings`).

## Testing

```sh
make run-tests      # cargo test: unit and integration tests
make debug-tests    # the same, showing each test's output
make run-mutants    # cargo mutants: mutation testing
```

### How the tests are organized

- **Unit tests** live in a `#[cfg(test)] mod tests` at the bottom of each module, written with `rstest`: every test is an `#[rstest]`, and tables of inputs use named cases like `#[case::reset(5_000, 100, 100)]`.
- **Shared helpers** are in `src/test_support.rs`, compiled only for tests: `CAPTURE` (the Linux 6.18 fixture), `TempFile` (a temp file that's removed on drop; give each test its own name, since tests run in parallel), `lines` (a ratatui `Buffer` as rows of text), and `Logs::capture` (what a closure logs, formatted like the log file). Chart tests also use `stats` and `render_lines` from the `test_support` module in `src/ui/charts/mod.rs`.
- **UI tests** render into ratatui's `TestBackend` or a `Buffer` and assert on the text and cell colors. There are no snapshot files.
- **The event loop** is tested by running `event_loop::run` on a `TestBackend`, over a temp copy of the fixture, with a scripted list of events in which `None` stands for a wait that timed out.
- **Time** is passed in, not read: `Ticker` takes `now` and `App::on_sample` takes `t`, so tests use made-up instants instead of sleeping.
- **Logging** tests capture output with a subscriber scoped to their own thread. Only `logs_to_the_file_and_starts_only_once` installs the process-wide subscriber, because that can happen once per test binary.
- **Integration tests** in `tests/` run the built binary (`cli.rs`), parse every real capture (`parse_captures.rs`), and read the machine's own `/proc/net/dev` (`procfs_live.rs`). That last one is compiled only on Linux, so `make run-tests` on macOS skips it, but CI runs it on every pull request. TODO: a `make` target that runs the tests in Docker, to run it locally too (AGENTS.md mentions `make test-linux`, which doesn't exist yet).
- **Fixtures** in `tests/fixtures/` are real captures, kept byte for byte. `linux-6.18.txt` comes from a Linux 6.18 VM and has `lo`, two `ifb` devices, and `eth0`. To add one, run `cat /proc/net/dev > tests/fixtures/<name>.txt` on the machine, and add a test for it to `tests/parse_captures.rs`.

### Edge cases

The parser's tests cover malformed lines, a missing header, too few or too many fields, values past 2^53, and names like `br-3f2a1b4c5d6e`, `eth0.100`, and `flannel.1`. The history's tests cover counter resets, 32-bit wraps, totals that would overflow, two readings at the same instant, and windows that fill up. The formatting tests cover each unit boundary and rates that are negative or NaN, and the legend is checked at every width from 0 to 199 columns.

### Mutation testing

`make run-mutants` runs [cargo-mutants](https://mutants.rs) over the whole crate: it makes small changes to the code, like replacing a function's body or flipping an operator, and checks that some test fails for each. Results go to `mutants.out/`, mainly `missed.txt`, `caught.txt`, `unviable.txt`, and `timeout.txt`, and the previous run moves to `mutants.out.old/`. Both are gitignored. For a quicker run on one file, use `cargo mutants --file src/history.rs`.

The goal is no missed mutants. A missed mutant gets a test that catches it or, when no test can, a `#[mutants::skip]` with a comment that says why. There are two so far: the compile-time check in `model.rs`, and `event_loop::crossterm_events`, which needs a real terminal. The `mutants` crate is a regular dependency so that the attribute compiles, and it does nothing at runtime.

## Per-release generated items

`docs/demo.gif` is the recording for the README. Record it again with `make record-demo` when the UI changes. The target builds the app and the traffic generator in release mode, then runs `docs/demo.tape` with [VHS](https://github.com/charmbracelet/vhs), which needs to be installed: `sudo port install vhs` with MacPorts, or `brew install vhs` on Apple silicon, since Homebrew no longer builds packages for Intel Macs. The tape's waits are timed to the generator's events, so change the two together.

## PR standards

A pull request should:

- Leave nothing for `make run-fmt` to change.
- Pass `cargo clippy --all-targets -- -W clippy::pedantic` with no warnings.
- Pass `make run-tests`.
- Add tests for new behavior, with no new missed mutants from `make run-mutants`.
- Update README.md and this file when it changes options, keys, setup, tooling, or the project's layout.

CI checks the first three on every pull request and every push to `main`, with `.github/workflows/ci.yml`. It runs `cargo fmt --check`, the clippy command with warnings made errors, and `cargo test --locked`, on Linux with Rust 1.99, the version the Dockerfile uses. Bump the two together.

## Ad hoc test scripts

`examples/demo_traffic.rs` writes a simulated `/proc/net/dev` 20 times a second, for trying the UI with busy, changing interfaces on any OS. Run it in one terminal and the app in another:

```sh
cargo run --release --example demo_traffic -- /tmp/proc-net-dev-demo.txt
cargo run --release -- --file /tmp/proc-net-dev-demo.txt -p 250 -w 20
```

It plays a host with steady traffic on `eth0`, a flaky `eth1` with errors, a VLAN with rare errors and a few drops, an idle `docker0`, a Docker bridge and its containers' veths, and a Kubernetes overlay. Counted from when it starts, a container starts at 8 s, another stops at 10.5 s, and `eth0` downloads for seven seconds at 22 s and again every minute. It rewrites the file in place at a fixed length, so the app never reads a short file. `make run-tests` runs its tests, which check that the app parses everything it writes and that the charts have something to show.
