//! Counts Enter taps for waking a full blackout.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct TapCounter {
    needed: usize,
    window: Duration,
    taps: VecDeque<Instant>,
}

impl TapCounter {
    pub fn new(needed: usize, window: Duration) -> Self {
        Self { needed, window, taps: VecDeque::new() }
    }

    /// Records a tap; true when it completes `needed` taps inside the time window.
    pub fn tap(&mut self) -> bool {
        self.tap_at(Instant::now())
    }

    fn tap_at(&mut self, now: Instant) -> bool {
        self.taps.push_back(now);
        while self.taps.front().is_some_and(|&first| now - first > self.window) {
            self.taps.pop_front();
        }
        if self.taps.len() < self.needed {
            return false;
        }
        self.taps.clear();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter() -> (TapCounter, Instant) {
        (TapCounter::new(3, Duration::from_secs(2)), Instant::now())
    }

    #[test]
    fn three_quick_taps_wake() {
        let (mut c, t) = counter();
        assert!(!c.tap_at(t));
        assert!(!c.tap_at(t + Duration::from_millis(300)));
        assert!(c.tap_at(t + Duration::from_millis(600)));
    }

    #[test]
    fn taps_spread_across_the_window_still_count() {
        let (mut c, t) = counter();
        c.tap_at(t);
        c.tap_at(t + Duration::from_millis(900));
        assert!(c.tap_at(t + Duration::from_millis(1800)));
    }

    #[test]
    fn a_long_gap_starts_over() {
        let (mut c, t) = counter();
        c.tap_at(t);
        c.tap_at(t + Duration::from_millis(100));
        assert!(!c.tap_at(t + Duration::from_millis(2200)));
        assert!(!c.tap_at(t + Duration::from_millis(2300)));
        assert!(c.tap_at(t + Duration::from_millis(2400)));
    }

    #[test]
    fn counting_restarts_after_a_wake() {
        let (mut c, t) = counter();
        for ms in [0, 100, 200] {
            c.tap_at(t + Duration::from_millis(ms));
        }
        assert!(!c.tap_at(t + Duration::from_millis(300)));
        assert!(!c.tap_at(t + Duration::from_millis(400)));
    }
}
