use std::time::Duration;

use jiff::Timestamp;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

pub fn between(then: Timestamp, now: Timestamp) -> Duration {
    let seconds = now.as_second().saturating_sub(then.as_second());
    Duration::from_secs(u64::try_from(seconds).unwrap_or_default())
}

pub fn since(then: Option<&Time>, now: Timestamp) -> Duration {
    then.map(|time| between(time.0, now)).unwrap_or_default()
}

pub fn age(meta: &ObjectMeta, now: Timestamp) -> Duration {
    since(meta.creation_timestamp.as_ref(), now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_and_missing_times_read_as_zero() {
        let now = Timestamp::from_second(1_000).unwrap_or_default();
        let past = Timestamp::from_second(940).unwrap_or_default();
        let later = Timestamp::from_second(2_000).unwrap_or_default();
        assert_eq!(between(past, now), Duration::from_secs(60));
        assert_eq!(between(later, now), Duration::ZERO);
        assert_eq!(since(None, now), Duration::ZERO);
    }
}
