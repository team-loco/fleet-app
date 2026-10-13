use std::time::Duration;

use tokio::time::Instant;

#[derive(Debug)]
pub struct Throttle {
    period: Duration,
    next: Instant,
}

impl Throttle {
    pub fn new(period: Duration) -> Self {
        Self {
            period,
            next: Instant::now(),
        }
    }

    pub async fn ready(&self) {
        tokio::time::sleep_until(self.next).await;
    }

    pub fn fired(&mut self) {
        self.next = Instant::now() + self.period;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: Duration = Duration::from_millis(250);
    const QUIET: Duration = Duration::from_secs(1);
    const STORM: Duration = Duration::from_secs(10);
    const STORM_STEP: Duration = Duration::from_millis(1);

    #[tokio::test(start_paused = true)]
    async fn first_change_after_quiet_passes_immediately() {
        let mut throttle = Throttle::new(PERIOD);
        let start = Instant::now();
        throttle.ready().await;
        assert_eq!(start.elapsed(), Duration::ZERO);
        throttle.fired();
        tokio::time::sleep(QUIET).await;
        let resumed = Instant::now();
        throttle.ready().await;
        assert_eq!(resumed.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn change_inside_the_period_waits_for_its_end() {
        let mut throttle = Throttle::new(PERIOD);
        throttle.fired();
        let start = Instant::now();
        throttle.ready().await;
        assert_eq!(start.elapsed(), PERIOD);
    }

    #[tokio::test(start_paused = true)]
    async fn storm_fires_at_most_once_per_period() {
        let mut throttle = Throttle::new(PERIOD);
        let start = Instant::now();
        let mut dirty = false;
        let mut fired = 0_u32;
        while start.elapsed() < STORM {
            tokio::select! {
                () = tokio::time::sleep(STORM_STEP) => dirty = true,
                () = throttle.ready(), if dirty => {
                    dirty = false;
                    throttle.fired();
                    fired += 1;
                }
            }
        }
        let periods = STORM.as_millis() / PERIOD.as_millis();
        assert_eq!(u128::from(fired), periods);
    }
}
