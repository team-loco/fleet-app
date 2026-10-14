use std::time::Duration;

const GROWTH_FACTOR: u32 = 2;

#[derive(Debug, Clone)]
pub struct Backoff {
    initial: Duration,
    max: Duration,
    next: Duration,
}

impl Backoff {
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max,
            next: initial,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        let grown = self.next.saturating_mul(GROWTH_FACTOR);
        self.next = grown.min(self.max);
        delay
    }

    pub fn reset(&mut self) {
        self.next = self.initial;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_until_capped() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(5));
        let delays: Vec<u64> = (0..5).map(|_| backoff.next_delay().as_secs()).collect();
        assert_eq!(delays, vec![1, 2, 4, 5, 5]);
    }

    #[test]
    fn reset_starts_over() {
        let mut backoff = Backoff::new(Duration::from_millis(100), Duration::from_secs(10));
        backoff.next_delay();
        backoff.next_delay();
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_millis(100));
    }
}
