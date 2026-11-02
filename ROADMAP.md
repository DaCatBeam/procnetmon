# Roadmap

Planned features and improvements, in no particular order.

- `--bits`, to show rates in bits per second (Mb/s), the unit network engineers usually read.
- `--chart <kind>`, to choose which chart opens first.
- Show the latest read or parse error on screen. The app already keeps it in `App.status`, but today only the log file shows it.
- Restore the terminal on `SIGTERM` and `SIGHUP`, not only on a quit, an error, or a panic.
- Zoomable long-term history, using downsampling.
- Prebuilt release binaries.
- Property-based tests for the parser.
