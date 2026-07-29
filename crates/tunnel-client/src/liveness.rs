use std::time::{Duration, Instant};

/// How long the link may stay silent (no inbound messages of any kind,
/// pongs included) after keepalive pings have started before it is declared
/// dead. Three 30s keepalive intervals: one lost pong is tolerated,
/// sustained silence is not. Before the first ping there is no expectation
/// of inbound traffic, so silence alone never kills a fresh connection.
pub const DEAD_AFTER: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Healthy,
    Dead,
}

/// Link-liveness evidence collector. Pure state: callers supply `now`, so
/// the logic is testable without a real clock.
pub struct LivenessTracker {
    last_evidence: Instant,
    ping_sent: bool,
}

impl LivenessTracker {
    pub fn new(now: Instant) -> LivenessTracker {
        LivenessTracker {
            last_evidence: now,
            ping_sent: false,
        }
    }

    pub fn on_ping_sent(&mut self) {
        self.ping_sent = true;
    }

    /// Any inbound WebSocket message counts as evidence the path is alive;
    /// data frames prove it as well as pongs do.
    pub fn on_traffic(&mut self, now: Instant) {
        self.last_evidence = now;
    }

    pub fn state(&self, now: Instant) -> LinkState {
        if self.ping_sent && now.duration_since(self.last_evidence) > DEAD_AFTER {
            LinkState::Dead
        } else {
            LinkState::Healthy
        }
    }

    pub fn silence(&self, now: Instant) -> Duration {
        now.duration_since(self.last_evidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn healthy_before_any_ping_even_after_long_silence() {
        let t0 = Instant::now();
        let tracker = LivenessTracker::new(t0);
        assert_eq!(
            tracker.state(t0 + Duration::from_secs(600)),
            LinkState::Healthy
        );
    }

    #[test]
    fn healthy_while_traffic_keeps_arriving() {
        let t0 = Instant::now();
        let mut tracker = LivenessTracker::new(t0);
        tracker.on_ping_sent();
        tracker.on_traffic(t0 + Duration::from_secs(60));
        assert_eq!(
            tracker.state(t0 + Duration::from_secs(120)),
            LinkState::Healthy
        );
    }

    #[test]
    fn dead_after_silence_exceeds_deadline_with_pings_sent() {
        let t0 = Instant::now();
        let mut tracker = LivenessTracker::new(t0);
        tracker.on_ping_sent();
        assert_eq!(tracker.state(t0 + DEAD_AFTER), LinkState::Healthy);
        assert_eq!(
            tracker.state(t0 + DEAD_AFTER + Duration::from_secs(1)),
            LinkState::Dead
        );
    }

    #[test]
    fn traffic_revives_a_silent_link() {
        let t0 = Instant::now();
        let mut tracker = LivenessTracker::new(t0);
        tracker.on_ping_sent();
        let late = t0 + DEAD_AFTER + Duration::from_secs(5);
        tracker.on_traffic(late);
        assert_eq!(
            tracker.state(late + Duration::from_secs(1)),
            LinkState::Healthy
        );
    }

    #[test]
    fn silence_reports_gap_since_last_evidence() {
        let t0 = Instant::now();
        let mut tracker = LivenessTracker::new(t0);
        tracker.on_traffic(t0 + Duration::from_secs(10));
        assert_eq!(
            tracker.silence(t0 + Duration::from_secs(40)),
            Duration::from_secs(30)
        );
    }
}
