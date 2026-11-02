use std::time::{Duration, Instant};

/// A fixed sampling schedule that doesn't drift, and doesn't burst to catch up after a stall.
///
/// `now` is always passed in, so tests can use made-up instants instead of sleeping.
pub struct Ticker {
    interval: Duration,
    next: Instant,
}

impl Ticker {
    /// The first tick is due at `now`.
    #[must_use]
    pub fn new(interval: Duration, now: Instant) -> Self {
        Self {
            interval,
            next: now,
        }
    }

    /// Returns `true` if a tick is due at `now`, and schedules the one after it.
    pub fn poll(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next += self.interval;
        if self.next <= now {
            self.next = now + self.interval;
        }
        true
    }

    /// How long until the next tick. Use it as the `event::poll` timeout.
    #[must_use]
    pub fn timeout(&self, now: Instant) -> Duration {
        self.next.saturating_duration_since(now)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    #[rstest]
    fn the_first_tick_is_due_at_once() {
        let t0 = Instant::now();
        let mut ticker = Ticker::new(SEC, t0);
        assert_eq!(ticker.timeout(t0), Duration::ZERO);
        assert!(ticker.poll(t0));
    }

    #[rstest]
    fn ticks_on_schedule_without_drifting() {
        let t0 = Instant::now();
        let mut ticker = Ticker::new(SEC, t0);
        assert!(ticker.poll(t0));
        assert!(!ticker.poll(t0 + SEC / 2));
        assert_eq!(ticker.timeout(t0 + SEC / 2), SEC / 2);
        // Handling a tick 30 ms late doesn't push the ones after it back.
        let late = t0 + SEC + Duration::from_millis(30);
        assert!(ticker.poll(late));
        assert_eq!(ticker.timeout(late), Duration::from_millis(970));
    }

    #[rstest]
    fn skips_missed_ticks_after_a_stall() {
        let t0 = Instant::now();
        let mut ticker = Ticker::new(SEC, t0);
        assert!(ticker.poll(t0));
        // Asleep for ten intervals: one tick on waking, not a burst of ten.
        let woke = t0 + 10 * SEC;
        assert!(ticker.poll(woke));
        assert!(!ticker.poll(woke));
        assert_eq!(ticker.timeout(woke), SEC);
    }
}
