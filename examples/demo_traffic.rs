//! Simulated traffic for recording the demo. It writes a made-up `/proc/net/dev` to a file 20
//! times a second, for the app to read with `--file`.
//!
//! The file plays a busy Linux host: steady traffic on `eth0`, a flaky `eth1` with errors, a VLAN
//! with rare errors and a few drops, an idle `docker0`, a Docker network's bridge and its
//! containers' veths, and a Kubernetes overlay. Counted from when the generator starts:
//!
//! - At 8 s a container starts, and its veth appears.
//! - At 10.5 s another container stops, and its veth disappears.
//! - At 22 s `eth0` starts a seven-second download, and it downloads again every minute.
//!
//! `docs/demo.tape` records the demo with it, timed to those events. To watch it yourself, run
//! these in two terminals:
//!
//! ```sh
//! cargo run --release --example demo_traffic -- /tmp/procnetmon-demo.txt
//! cargo run --release -- --file /tmp/procnetmon-demo.txt -p 250 -w 20
//! ```

use std::{
    env,
    f64::consts::TAU,
    fmt::Write as _,
    fs::File,
    io::{self, Seek, Write},
    iter,
    ops::Range,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

/// Where the counters go when no path is given.
const DEFAULT_PATH: &str = "/tmp/procnetmon-demo.txt";

/// How often the file is rewritten. The app's samples, every 250 ms in the demo, see counters
/// that are at most this old.
const TICK: Duration = Duration::from_millis(50);

/// When a container starts, in seconds after the generator does.
const NEW_CONTAINER_AT: f64 = 8.0;
/// When another container stops.
const OLD_CONTAINER_STOPS_AT: f64 = 10.5;
/// When `eth0`'s first download starts.
const DOWNLOAD_AT: f64 = 22.0;
/// How often a download repeats.
const DOWNLOAD_EVERY: f64 = 60.0;

/// When each kind of interface exists, in seconds after the start.
const ALWAYS: Range<f64> = 0.0..f64::INFINITY;
const OLD_CONTAINER: Range<f64> = 0.0..OLD_CONTAINER_STOPS_AT;
const NEW_CONTAINER: Range<f64> = NEW_CONTAINER_AT..f64::INFINITY;

const KIB: f64 = 1024.0;
const MIB: f64 = 1024.0 * KIB;

/// The kernel's two header lines, which the app checks for.
const HEADER: &str = concat!(
    "Inter-|   Receive                                                |  Transmit\n",
    " face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n",
);

// The kernel's columns that the simulation counts, by position. The others stay at zero.
const RX_BYTES: usize = 0;
const RX_PACKETS: usize = 1;
const RX_ERRS: usize = 2;
const RX_DROP: usize = 3;
const RX_MULTICAST: usize = 7;
const TX_BYTES: usize = 8;
const TX_PACKETS: usize = 9;
const TX_ERRS: usize = 10;
const TX_DROP: usize = 11;
/// Counters per interface.
const COLUMNS: usize = 16;

/// Linux interface names are at most 15 bytes.
const NAME_WIDTH: usize = 15;
/// `u64::MAX` has 20 digits.
const NUMBER_WIDTH: usize = 20;
/// Every interface gets a line this long, blank while it doesn't exist, so the file always has
/// the same length.
const LINE_WIDTH: usize = NAME_WIDTH + 1 + COLUMNS * (1 + NUMBER_WIDTH);

fn main() -> io::Result<()> {
    let path = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from(DEFAULT_PATH), PathBuf::from);
    let mut file = File::create(&path)?;
    eprintln!(
        "Writing simulated counters to {} every {} ms. Stop with Ctrl+C.",
        path.display(),
        TICK.as_millis()
    );

    let mut links = host();
    let mut contents = String::new();
    let start = Instant::now();
    let mut due = start;
    let mut last = 0.0;
    loop {
        let t = start.elapsed().as_secs_f64();
        for link in &mut links {
            link.advance(t, t - last);
        }
        last = t;
        render(&links, t, &mut contents);
        // Rewritten in place, because the app keeps the file open and wouldn't see a new file put
        // in its place. And since the length never changes, a read that lands mid-write never sees
        // a short file.
        file.rewind()?;
        file.write_all(contents.as_bytes())?;

        due += TICK;
        match due.checked_duration_since(Instant::now()) {
            Some(wait) => thread::sleep(wait),
            // Running behind: carry on from now rather than rushing to catch up.
            None => due = Instant::now(),
        }
    }
}

/// The host's interfaces, in the order the kernel lists them.
fn host() -> Vec<Link> {
    vec![
        Link::new(
            "lo",
            ALWAYS,
            Direction::new(loopback, 900.0),
            Direction::new(loopback, 900.0),
        )
        .since_boot([812_331_904.0, 902_591.0, 812_331_904.0, 902_591.0]),
        Link::new(
            "eth0",
            ALWAYS,
            Direction::new(eth0_in, 1_380.0),
            Direction::new(eth0_out, 160.0),
        )
        .multicast(1.5)
        .since_boot([
            48_213_901_212.0,
            35_119_402.0,
            6_402_118_771.0,
            21_884_390.0,
        ]),
        Link::new(
            "eth1",
            ALWAYS,
            Direction::new(eth1_in, 900.0).errors(0.006).drops(0.002),
            Direction::new(eth1_out, 600.0),
        )
        .since_boot([3_118_204_118.0, 3_464_671.0, 1_207_559_310.0, 2_012_598.0]),
        Link::new(
            "eth0.100",
            ALWAYS,
            Direction::new(vlan_in, 800.0).errors(0.000_3).drops(0.001),
            Direction::new(vlan_out, 500.0),
        )
        .multicast(0.2)
        .since_boot([9_844_120_557.0, 12_305_150.0, 2_710_334_902.0, 5_420_669.0]),
        Link::new(
            "docker0",
            ALWAYS,
            Direction::new(idle, 100.0),
            Direction::new(idle, 100.0),
        )
        .since_boot([0.0, 0.0, 7_812.0, 92.0]),
        Link::new(
            "br-3f2a1b4c5d6e",
            ALWAYS,
            Direction::new(bridge_in, 1_100.0),
            Direction::new(bridge_out, 700.0),
        )
        .since_boot([1_930_441_820.0, 1_754_947.0, 7_412_009_315.0, 10_588_584.0]),
        // A container's veth sees the container's side flipped: what it sends arrives here.
        Link::new(
            "veth4c1e2a7",
            OLD_CONTAINER,
            Direction::new(web_sends, 1_200.0),
            Direction::new(web_receives, 300.0),
        )
        .since_boot([4_611_872_090.0, 3_843_226.0, 1_102_337_461.0, 3_674_458.0]),
        Link::new(
            "flannel.1",
            ALWAYS,
            Direction::new(overlay_in, 1_100.0),
            Direction::new(overlay_out, 1_000.0).drops(0.000_5),
        )
        .since_boot([5_203_118_944.0, 4_730_108.0, 4_811_207_651.0, 4_811_207.0]),
        // Starts at zero, like any new veth.
        Link::new(
            "vethb93d5f0",
            NEW_CONTAINER,
            Direction::new(worker_sends, 900.0),
            Direction::new(worker_receives, 600.0),
        ),
    ]
}

/// A simulated interface.
struct Link {
    name: &'static str,
    /// Seconds after the start during which it exists.
    lives: Range<f64>,
    rx: Direction,
    tx: Direction,
    /// Multicast frames received per second.
    multicast: f64,
    /// Its counters, as `f64` so that fractions of a frame add up over the ticks.
    counters: [f64; COLUMNS],
}

impl Link {
    fn new(name: &'static str, lives: Range<f64>, rx: Direction, tx: Direction) -> Self {
        Self {
            name,
            lives,
            rx,
            tx,
            multicast: 0.0,
            counters: [0.0; COLUMNS],
        }
    }

    fn multicast(self, per_sec: f64) -> Self {
        Self {
            multicast: per_sec,
            ..self
        }
    }

    /// Counters at the start, as if the host had been up a while: bytes and packets each way.
    fn since_boot(mut self, [rx_bytes, rx_packets, tx_bytes, tx_packets]: [f64; 4]) -> Self {
        self.counters[RX_BYTES] = rx_bytes;
        self.counters[RX_PACKETS] = rx_packets;
        self.counters[TX_BYTES] = tx_bytes;
        self.counters[TX_PACKETS] = tx_packets;
        self
    }

    fn exists_at(&self, t: f64) -> bool {
        self.lives.contains(&t)
    }

    /// Counts the traffic from `dt` seconds before `t` until `t`.
    fn advance(&mut self, t: f64, dt: f64) {
        if !self.exists_at(t) {
            return;
        }
        let mid = t - dt / 2.0;
        let directions = [
            (self.rx, [RX_BYTES, RX_PACKETS, RX_ERRS, RX_DROP]),
            (self.tx, [TX_BYTES, TX_PACKETS, TX_ERRS, TX_DROP]),
        ];
        for (direction, [bytes, packets, errors, drops]) in directions {
            let moved = (direction.rate)(mid).max(0.0) * dt;
            let frames = moved / direction.frame_size;
            self.counters[bytes] += moved;
            // Like the kernel's, the packet counters count only good frames.
            self.counters[packets] += frames * (1.0 - direction.errors);
            self.counters[errors] += frames * direction.errors;
            self.counters[drops] += frames * direction.drops;
        }
        self.counters[RX_MULTICAST] += self.multicast * dt;
    }
}

/// One direction of an interface's traffic.
#[derive(Clone, Copy)]
struct Direction {
    /// Bytes per second, `t` seconds after the start.
    rate: fn(f64) -> f64,
    /// Average frame size, in bytes.
    frame_size: f64,
    /// The share of frames that are errors.
    errors: f64,
    /// The share of frames that are dropped.
    drops: f64,
}

impl Direction {
    fn new(rate: fn(f64) -> f64, frame_size: f64) -> Self {
        Self {
            rate,
            frame_size,
            errors: 0.0,
            drops: 0.0,
        }
    }

    fn errors(self, share: f64) -> Self {
        Self {
            errors: share,
            ..self
        }
    }

    fn drops(self, share: f64) -> Self {
        Self {
            drops: share,
            ..self
        }
    }
}

/// Writes the counters the way the kernel lays them out, one line per interface.
fn render(links: &[Link], t: f64, out: &mut String) {
    out.clear();
    out.push_str(HEADER);
    for link in links {
        let start = out.len();
        if link.exists_at(t) {
            write!(out, "{:>NAME_WIDTH$}:", link.name).expect("a String takes any text");
            for &value in &link.counters {
                write!(out, " {:>NUMBER_WIDTH$}", whole(value)).expect("a String takes any text");
            }
        }
        // A missing interface leaves a blank line, which the app skips.
        let padding = LINE_WIDTH.saturating_sub(out.len() - start);
        out.extend(iter::repeat_n(' ', padding));
        out.push('\n');
    }
}

/// A counter as the whole number the kernel prints.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "counters are positive and far below u64::MAX, and `as` saturates anyway"
)]
fn whole(value: f64) -> u64 {
    value as u64
}

// Traffic, in bytes per second, `t` seconds after the start.

fn loopback(t: f64) -> f64 {
    26.0 * KIB + 14.0 * KIB * wave(t, 9.0, 0.0) + 6.0 * KIB * noise(1, t)
}

fn eth0_in(t: f64) -> f64 {
    let steady = 3.2 * MIB + 1.3 * MIB * wave(t, 23.0, 0.5) + 0.6 * MIB * wave(t, 7.3, 2.0);
    let download = 17.0 * MIB * download(t) * (1.0 + 0.1 * noise(3, 4.0 * t));
    steady + 0.5 * MIB * noise(2, t) + download
}

/// Mostly acknowledgements, which grow with a download.
fn eth0_out(t: f64) -> f64 {
    420.0 * KIB
        + 160.0 * KIB * wave(t, 13.0, 1.0)
        + 80.0 * KIB * noise(4, t)
        + 0.6 * MIB * download(t)
}

fn eth1_in(t: f64) -> f64 {
    380.0 * KIB + 110.0 * KIB * wave(t, 17.0, 0.3) + 60.0 * KIB * noise(5, t)
}

fn eth1_out(t: f64) -> f64 {
    150.0 * KIB + 50.0 * KIB * wave(t, 11.0, 2.2) + 30.0 * KIB * noise(6, t)
}

fn vlan_in(t: f64) -> f64 {
    640.0 * KIB + 200.0 * KIB * wave(t, 29.0, 1.7) + 70.0 * KIB * noise(7, t)
}

fn vlan_out(t: f64) -> f64 {
    210.0 * KIB + 60.0 * KIB * wave(t, 19.0, 0.9) + 25.0 * KIB * noise(8, t)
}

fn idle(_: f64) -> f64 {
    0.0
}

/// A web server's responses.
fn web_sends(t: f64) -> f64 {
    2.4 * MIB + 0.8 * MIB * wave(t, 11.0, 0.8) + 0.3 * MIB * noise(9, t)
}

/// The requests it answers.
fn web_receives(t: f64) -> f64 {
    600.0 * KIB + 150.0 * KIB * wave(t, 11.0, 0.8) + 60.0 * KIB * noise(10, t)
}

/// A worker that pulls in data, ramping up over its first two seconds.
fn worker_sends(t: f64) -> f64 {
    starting(t) * (350.0 * KIB + 100.0 * KIB * wave(t, 7.0, 0.0) + 50.0 * KIB * noise(11, t))
}

fn worker_receives(t: f64) -> f64 {
    starting(t) * (1.2 * MIB + 0.4 * MIB * wave(t, 7.0, 1.2) + 0.2 * MIB * noise(12, t))
}

/// The bridge carries whatever its containers send.
fn bridge_in(t: f64) -> f64 {
    40.0 * KIB + while_in(&OLD_CONTAINER, t, web_sends) + while_in(&NEW_CONTAINER, t, worker_sends)
}

/// And whatever they receive.
fn bridge_out(t: f64) -> f64 {
    30.0 * KIB
        + while_in(&OLD_CONTAINER, t, web_receives)
        + while_in(&NEW_CONTAINER, t, worker_receives)
}

fn overlay_in(t: f64) -> f64 {
    1.1 * MIB + 0.5 * MIB * wave(t, 19.0, 2.4) + 0.15 * MIB * noise(13, t)
}

fn overlay_out(t: f64) -> f64 {
    0.9 * MIB + 0.4 * MIB * wave(t, 19.0, 2.9) + 0.12 * MIB * noise(14, t)
}

// The shapes the traffic is made of.

/// `rate` at `t` while `t` is in `lives`, and nothing otherwise.
fn while_in(lives: &Range<f64>, t: f64, rate: fn(f64) -> f64) -> f64 {
    if lives.contains(&t) { rate(t) } else { 0.0 }
}

/// A wave between -1 and 1 that repeats every `period` seconds.
fn wave(t: f64, period: f64, phase: f64) -> f64 {
    (TAU * t / period + phase).sin()
}

/// How far into a download `t` is: 0 outside one, easing up to 1 and holding for four seconds,
/// then easing back down.
fn download(t: f64) -> f64 {
    let into = (t - DOWNLOAD_AT).rem_euclid(DOWNLOAD_EVERY);
    if into < 1.5 {
        smooth(into / 1.5)
    } else if into < 5.5 {
        1.0
    } else if into < 7.0 {
        smooth((7.0 - into) / 1.5)
    } else {
        0.0
    }
}

/// How far the new container is into starting up, from 0 to 1 over two seconds.
fn starting(t: f64) -> f64 {
    smooth(((t - NEW_CONTAINER_AT) / 2.0).clamp(0.0, 1.0))
}

/// Eases from 0 to 1 as `x` goes from 0 to 1.
fn smooth(x: f64) -> f64 {
    x * x * (3.0 - 2.0 * x)
}

/// Smooth noise between -1 and 1 that wanders about once a second, the same on every run.
fn noise(seed: u64, t: f64) -> f64 {
    let step = t.floor();
    let from = random(seed, step);
    let to = random(seed, step + 1.0);
    from + (to - from) * smooth(t - step)
}

/// A random number between -1 and 1 for `seed` and `step`, from a `SplitMix64` hash.
fn random(seed: u64, step: f64) -> f64 {
    let mut z = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ step.to_bits();
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let high = u32::try_from(z >> 32).expect("the top 32 bits fit in a u32");
    f64::from(high) / f64::from(u32::MAX) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use procnetmon::{
        history::History,
        model::Counter,
        parse::parse,
        ui::charts::health::{Health, Severity},
    };
    use rstest::rstest;

    use super::*;

    /// The file `t` seconds after the start, with the counters still at their starting values.
    fn contents_at(t: f64) -> String {
        let mut contents = String::new();
        render(&host(), t, &mut contents);
        contents
    }

    /// The interfaces the app reads from the file `t` seconds after the start.
    fn names_at(t: f64) -> Vec<String> {
        parse(&contents_at(t))
            .map(|stats| stats.expect("the app parses every line").name.to_owned())
            .collect()
    }

    /// Plays the first `secs` seconds tick by tick, and records what the app sees when it samples
    /// every 250 ms over a 20-second window, as in the demo.
    fn play(secs: f64) -> History {
        let mut links = host();
        let mut history = History::new(81);
        let mut contents = String::new();
        let dt = TICK.as_secs_f64();
        let mut t = 0.0;
        let mut ticks = 0_u32;
        while t < secs {
            t += dt;
            ticks += 1;
            for link in &mut links {
                link.advance(t, dt);
            }
            if ticks.is_multiple_of(5) {
                render(&links, t, &mut contents);
                for stats in parse(&contents) {
                    history.record(t, stats.expect("the app parses every line"));
                }
            }
        }
        history
    }

    #[rstest]
    #[case::at_the_start(0.0, true, false)]
    #[case::once_the_new_container_starts(NEW_CONTAINER_AT, true, true)]
    #[case::once_the_old_one_stops(OLD_CONTAINER_STOPS_AT, false, true)]
    fn containers_come_and_go(#[case] t: f64, #[case] old: bool, #[case] new: bool) {
        let names = names_at(t);
        assert!(names.iter().any(|name| name == "eth0"), "{names:?}");
        assert_eq!(
            names.iter().any(|name| name == "veth4c1e2a7"),
            old,
            "{names:?}"
        );
        assert_eq!(
            names.iter().any(|name| name == "vethb93d5f0"),
            new,
            "{names:?}"
        );
    }

    #[rstest]
    fn the_file_keeps_its_length() {
        let length = contents_at(0.0).len();
        // One veth has gone, and another has come.
        assert_eq!(contents_at(OLD_CONTAINER_STOPS_AT).len(), length);

        // However big the counters get.
        let mut links = host();
        for link in &mut links {
            link.counters = [f64::MAX; COLUMNS];
        }
        let mut contents = String::new();
        render(&links, NEW_CONTAINER_AT, &mut contents);
        assert_eq!(contents.len(), length);
        assert!(parse(&contents).all(|stats| stats.is_ok()), "{contents}");
    }

    #[rstest]
    fn eth0_downloads_on_cue() {
        let history = play(DOWNLOAD_AT + 6.0);
        let eth0 = history.get("eth0").expect("eth0 exists");
        let peak = |from: f64, to: f64| {
            eth0.rates(Counter::RxBytes)
                .filter(|&(t, _)| (from..to).contains(&t))
                .map(|(_, rate)| rate)
                .fold(0.0, f64::max)
        };
        let before = peak(DOWNLOAD_AT - 10.0, DOWNLOAD_AT);
        let during = peak(DOWNLOAD_AT + 2.0, DOWNLOAD_AT + 5.0);
        assert!(during > 3.0 * before, "{before} B/s, then {during} B/s");
    }

    #[rstest]
    #[case::clean_uplink("eth0", Severity::Clean, false)]
    #[case::flaky_link("eth1", Severity::Bad, true)]
    #[case::vlan_with_rare_errors("eth0.100", Severity::Warn, true)]
    #[case::overlay_dropping_a_few("flannel.1", Severity::Clean, true)]
    #[case::bridge_without_containers("docker0", Severity::Idle, false)]
    fn errors_and_drops_have_something_to_show(
        #[case] name: &str,
        #[case] severity: Severity,
        #[case] drops: bool,
    ) {
        let history = play(20.0);
        let interface = history.get(name).expect("it exists");
        let health = Health::of(interface).expect("20 seconds to average over");
        assert_eq!(Severity::of(health.error_ratio), severity, "{health:?}");
        assert_eq!(health.drops > 0.0, drops, "{health:?}");
    }
}
