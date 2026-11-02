//! Sample history for every interface: a bounded window of readings per interface, plus the
//! per-second rates and session totals the charts are drawn from.

use crate::model::{Counter, Counters, InterfaceStats};
use std::collections::VecDeque;

/// One reading of an interface's counters.
///
/// It's 136 bytes. At the default 5-minute window and 1-second interval, each interface keeps 301
/// of them, about 40 KB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Seconds since the app started, measured with `Instant`. This is the chart's x coordinate.
    pub t: f64,
    /// The interface's counters at `t`.
    pub counters: Counters,
}

/// Every interface's history, in the order the interfaces first appeared.
///
/// Interfaces are never removed, so an index into [`History::iter`] keeps pointing at the same
/// interface, even when a new one shows up mid-session. Lookups by name are linear, which is
/// plenty fast for the few dozen interfaces a host has.
#[derive(Debug)]
pub struct History {
    /// Samples kept per interface; shared by all interfaces.
    capacity: usize,
    interfaces: Vec<(String, InterfaceHistory)>,
}

impl History {
    /// Creates an empty history that keeps `capacity` samples per interface.
    ///
    /// Use `window / poll_interval + 1` to cover the whole chart window. A rate needs two
    /// samples, so a `capacity` below 2 is raised to 2.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(2),
            interfaces: Vec::new(),
        }
    }

    /// Adds one parsed line, read `t` seconds after the app started.
    ///
    /// The first reading of an interface allocates its sample buffer and copies its name. After
    /// that, recording doesn't allocate.
    pub fn record(&mut self, t: f64, stats: InterfaceStats<'_>) {
        let sample = Sample {
            t,
            counters: stats.counters,
        };
        let seen = self.interfaces.iter_mut().find(|(n, _)| n == stats.name);
        if let Some((_, history)) = seen {
            history.push(sample, self.capacity);
        } else {
            let mut history = InterfaceHistory::new(self.capacity);
            history.push(sample, self.capacity);
            self.interfaces.push((stats.name.to_owned(), history));
        }
    }

    /// The history of the interface called `name`, if it has been recorded.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&InterfaceHistory> {
        self.iter()
            .find_map(|(n, history)| (n == name).then_some(history))
    }

    /// Interface names in first-seen order. They're borrowed, so a list widget doesn't clone them.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.iter().map(|(name, _)| name)
    }

    /// `(name, history)` pairs in first-seen order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &InterfaceHistory)> {
        self.interfaces
            .iter()
            .map(|(name, history)| (name.as_str(), history))
    }
}

/// The newest samples of one interface, plus its totals since the app started.
#[derive(Debug)]
pub struct InterfaceHistory {
    /// Oldest first, and never more than the history's capacity.
    samples: VecDeque<Sample>,
    /// Running totals since startup (the "volume" view); not bounded by the window.
    session: Counters,
}

impl InterfaceHistory {
    fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            session: Counters::default(),
        }
    }

    fn push(&mut self, sample: Sample, capacity: usize) {
        if let Some(prev) = self.samples.back() {
            for counter in Counter::ALL {
                let increase = delta(prev.counters[counter], sample.counters[counter]);
                // Saturating, so a driver that reports garbage can't overflow the total (which
                // would panic in a debug build).
                self.session[counter] = self.session[counter].saturating_add(increase);
            }
        }
        if self.samples.len() == capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// Per-second rate of `counter`, as `(t, rate)` chart points.
    ///
    /// There's one point per pair of neighboring samples, placed at the later sample's `t`. A pair
    /// with no time between them is skipped rather than drawn as an infinite rate. Points come
    /// oldest first, and `next_back()` gives the newest without walking the window.
    pub fn rates(&self, counter: Counter) -> impl DoubleEndedIterator<Item = (f64, f64)> {
        self.samples
            .iter()
            .zip(self.samples.iter().skip(1))
            .filter_map(move |(prev, next)| {
                let elapsed = next.t - prev.t;
                let increase = delta(prev.counters[counter], next.counters[counter]);
                (elapsed > 0.0).then(|| (next.t, as_f64(increase) / elapsed))
            })
    }

    /// How much `counter` grew since the interface was first seen, for example bytes received
    /// this session.
    ///
    /// The first sample is only a baseline. Counter resets don't lose the total, and it isn't
    /// limited to the chart window.
    #[must_use]
    pub fn session_total(&self, counter: Counter) -> u64 {
        self.session[counter]
    }

    /// How much `counter` grew across the samples in the window, counting resets like [`delta`].
    #[must_use]
    pub fn window_total(&self, counter: Counter) -> u64 {
        self.samples
            .iter()
            .zip(self.samples.iter().skip(1))
            .map(|(prev, next)| delta(prev.counters[counter], next.counters[counter]))
            .fold(0, u64::saturating_add)
    }

    /// Seconds from the oldest sample in the window to the newest, once there are two.
    #[must_use]
    pub fn span_secs(&self) -> Option<f64> {
        match (self.samples.front(), self.samples.back()) {
            (Some(oldest), Some(newest)) if self.samples.len() > 1 => Some(newest.t - oldest.t),
            _ => None,
        }
    }

    /// The newest sample. Its `t` is where the chart's x-axis ends.
    #[must_use]
    pub fn latest(&self) -> Option<&Sample> {
        self.samples.back()
    }
}

/// Increase between two readings of a monotonic counter. A decrease means the interface was
/// re-created or reset, so the new value is everything counted since the reset.
#[must_use]
pub const fn delta(prev: u64, next: u64) -> u64 {
    match next.checked_sub(prev) {
        Some(d) => d,
        None => next,
    }
}

/// Converts a count to a chart coordinate.
#[expect(
    clippy::cast_precision_loss,
    reason = "chart coordinates; per-interval deltas are far below 2^53"
)]
#[must_use]
pub const fn as_f64(value: u64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Past 2^53, where `f64` can't represent every integer.
    const HUGE: u64 = 1 << 60;

    /// A parsed line with only `rx_bytes` set.
    fn stats(name: &str, rx_bytes: u64) -> InterfaceStats<'_> {
        let mut counters = Counters::default();
        counters[Counter::RxBytes] = rx_bytes;
        InterfaceStats { name, counters }
    }

    /// A history fed `(t, rx_bytes)` readings of eth0.
    fn eth0_history(capacity: usize, readings: &[(f64, u64)]) -> History {
        let mut history = History::new(capacity);
        for &(t, rx_bytes) in readings {
            history.record(t, stats("eth0", rx_bytes));
        }
        history
    }

    fn interface<'a>(history: &'a History, name: &str) -> &'a InterfaceHistory {
        history.get(name).expect("interface was recorded")
    }

    fn rx_rates(history: &History, name: &str) -> Vec<(f64, f64)> {
        interface(history, name).rates(Counter::RxBytes).collect()
    }

    fn rx_total(history: &History, name: &str) -> u64 {
        interface(history, name).session_total(Counter::RxBytes)
    }

    #[rstest]
    #[case::one_sample_has_no_rate(&[(0.0, 100)], &[])]
    #[case::divides_by_the_real_elapsed_time(
        &[(0.0, 0), (0.5, 1_000), (1.5, 3_000)],
        &[(0.5, 2_000.0), (1.5, 2_000.0)]
    )]
    #[case::skips_readings_taken_at_the_same_time(
        &[(0.0, 0), (0.0, 500), (1.0, 1_500)],
        &[(1.0, 1_000.0)]
    )]
    #[case::counts_a_reset_from_zero(&[(0.0, 5_000), (1.0, 100)], &[(1.0, 100.0)])]
    #[case::stays_exact_past_2_pow_53(&[(0.0, HUGE), (1.0, HUGE + 3)], &[(1.0, 3.0)])]
    fn rx_rates_are_per_second(#[case] readings: &[(f64, u64)], #[case] expected: &[(f64, f64)]) {
        assert_eq!(rx_rates(&eth0_history(10, readings), "eth0"), expected);
    }

    #[rstest]
    fn session_totals_start_at_zero_and_survive_resets() {
        // eth0 is re-created between the second and third readings.
        let history = eth0_history(10, &[(0.0, 1_000), (1.0, 1_500), (2.0, 200)]);
        // The first reading is only a baseline: 500 before the reset, plus 200 after it.
        assert_eq!(rx_total(&history, "eth0"), 700);
    }

    #[rstest]
    fn session_totals_saturate_instead_of_overflowing() {
        // A misbehaving driver jumps to the top of the range, then resets.
        let history = eth0_history(10, &[(0.0, 0), (1.0, u64::MAX), (2.0, 5)]);
        assert_eq!(rx_total(&history, "eth0"), u64::MAX);
    }

    #[rstest]
    fn window_is_bounded_but_session_totals_are_not() {
        let readings: Vec<_> = (0..100_u32)
            .map(|i| (f64::from(i), u64::from(i) * 10))
            .collect();
        let history = eth0_history(3, &readings);
        // Only the newest three samples are left, so there are two rates.
        assert_eq!(rx_rates(&history, "eth0"), [(98.0, 10.0), (99.0, 10.0)]);
        let newest = interface(&history, "eth0").latest().map(|s| s.t);
        assert_eq!(newest, Some(99.0));
        assert_eq!(rx_total(&history, "eth0"), 990);
    }

    #[rstest]
    fn rates_read_newest_first_from_the_back() {
        let history = eth0_history(10, &[(0.0, 0), (1.0, 100), (3.0, 500)]);
        let eth0 = interface(&history, "eth0");
        assert_eq!(eth0.rates(Counter::RxBytes).next_back(), Some((3.0, 200.0)));
        let newest_first: Vec<_> = eth0.rates(Counter::RxBytes).rev().collect();
        assert_eq!(newest_first, [(3.0, 200.0), (1.0, 100.0)]);
    }

    #[rstest]
    fn window_totals_cover_only_the_window() {
        // Capacity 3 keeps the readings at t = 97, 98 and 99, and eth0 is re-created before 99.
        let mut readings: Vec<_> = (0..99_u32)
            .map(|i| (f64::from(i), u64::from(i) * 10))
            .collect();
        readings.push((99.0, 4));
        let history = eth0_history(3, &readings);
        let eth0 = interface(&history, "eth0");
        assert_eq!(eth0.window_total(Counter::RxBytes), 10 + 4);
        assert_eq!(eth0.span_secs(), Some(2.0));
    }

    #[rstest]
    fn a_window_needs_two_samples() {
        let history = eth0_history(10, &[(5.0, 100)]);
        let eth0 = interface(&history, "eth0");
        assert_eq!(eth0.window_total(Counter::RxBytes), 0);
        assert_eq!(eth0.span_secs(), None);
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(2)]
    fn capacity_is_at_least_two_samples(#[case] capacity: usize) {
        let history = eth0_history(capacity, &[(0.0, 0), (1.0, 10), (2.0, 30)]);
        assert_eq!(rx_rates(&history, "eth0"), [(2.0, 20.0)]);
    }

    #[rstest]
    fn each_counter_has_its_own_rate_and_total() {
        // Between the two readings, rx_bytes grows by 1, rx_packets by 2, and so on.
        let name = "eth0";
        let mut counters = Counters::default();
        let mut history = History::new(10);
        history.record(0.0, InterfaceStats { name, counters });
        for (counter, growth) in Counter::ALL.into_iter().zip(1_u64..) {
            counters[counter] = growth;
        }
        history.record(1.0, InterfaceStats { name, counters });

        let eth0 = interface(&history, name);
        for (counter, growth) in Counter::ALL.into_iter().zip(1_u64..) {
            assert_eq!(eth0.session_total(counter), growth, "{counter:?}");
            let rates: Vec<_> = eth0.rates(counter).collect();
            assert_eq!(rates, [(1.0, as_f64(growth))], "{counter:?}");
        }
    }

    #[rstest]
    fn readings_go_to_their_own_interface() {
        let mut history = History::new(10);
        for (t, eth0_rx, wlan0_rx) in [(0.0, 0, 1_000), (1.0, 100, 4_000)] {
            history.record(t, stats("eth0", eth0_rx));
            history.record(t, stats("wlan0", wlan0_rx));
        }
        assert_eq!(rx_rates(&history, "eth0"), [(1.0, 100.0)]);
        assert_eq!(rx_rates(&history, "wlan0"), [(1.0, 3_000.0)]);
        assert!(history.get("docker0").is_none());
    }

    #[rstest]
    fn interfaces_keep_their_first_seen_position() {
        let mut history = History::new(10);
        for name in ["lo", "eth0"] {
            history.record(0.0, stats(name, 0));
        }
        // A new interface can show up anywhere in the next read.
        for name in ["lo", "br-3f2a1b4c5d6e", "eth0"] {
            history.record(1.0, stats(name, 0));
        }
        let names: Vec<_> = history.names().collect();
        assert_eq!(names, ["lo", "eth0", "br-3f2a1b4c5d6e"]);
        // So the interface list's selected index still points at the same interface.
        assert_eq!(history.iter().nth(1).map(|(name, _)| name), Some("eth0"));
    }

    #[rstest]
    #[case::increase(100, 150, 50)]
    #[case::unchanged(100, 100, 0)]
    #[case::reset(5_000, 100, 100)]
    #[case::reset_to_zero(5_000, 0, 0)]
    #[case::whole_range(0, u64::MAX, u64::MAX)]
    #[case::u32_wrap_counts_as_a_reset(u64::from(u32::MAX) - 10, 5, 5)]
    fn delta_between_readings(#[case] prev: u64, #[case] next: u64, #[case] expected: u64) {
        assert_eq!(delta(prev, next), expected);
    }
}
