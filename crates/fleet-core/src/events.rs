use std::time::Duration;

use crate::Status;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub status: Status,
    pub reason: String,
    pub object: String,
    pub namespace: Option<String>,
    pub message: String,
    pub age: Duration,
    pub count: u32,
}

impl Event {
    pub fn is_warning(&self) -> bool {
        self.status != Status::Ok
    }

    fn matches(&self, filter: EventFilter, query: &str) -> bool {
        let kept = match filter {
            EventFilter::All => true,
            EventFilter::Warnings => self.is_warning(),
            EventFilter::Normal => !self.is_warning(),
        };
        let query = query.trim().to_lowercase();
        let text_match = query.is_empty()
            || [
                self.reason.as_str(),
                self.object.as_str(),
                self.message.as_str(),
                self.namespace.as_deref().unwrap_or_default(),
            ]
            .iter()
            .any(|field| field.to_lowercase().contains(&query));
        kept && text_match
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventFilter {
    #[default]
    All,
    Warnings,
    Normal,
}

impl EventFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::Warnings, Self::Normal];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Warnings => "Warnings",
            Self::Normal => "Normal",
        }
    }
}

pub fn warnings_first(events: &[Event]) -> Vec<Event> {
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| {
        b.is_warning()
            .cmp(&a.is_warning())
            .then(b.status.cmp(&a.status))
            .then(a.age.cmp(&b.age))
    });
    sorted
}

pub fn recent_warnings(events: &[Event], limit: usize) -> Vec<Event> {
    let mut warnings: Vec<Event> = events.iter().filter(|e| e.is_warning()).cloned().collect();
    warnings.sort_by_key(|e| e.age);
    warnings.truncate(limit);
    warnings
}

pub fn filter_events(events: &[Event], filter: EventFilter, query: &str) -> Vec<Event> {
    let kept: Vec<Event> = events
        .iter()
        .filter(|e| e.matches(filter, query))
        .cloned()
        .collect();
    warnings_first(&kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60;

    fn event(status: Status, reason: &str, minutes: u64) -> Event {
        Event {
            status,
            reason: reason.into(),
            object: format!("pod/{}", reason.to_lowercase()),
            namespace: Some("payments".into()),
            message: format!("{reason} happened"),
            age: Duration::from_secs(minutes * MINUTE),
            count: 1,
        }
    }

    fn reasons(events: &[Event]) -> Vec<&str> {
        events.iter().map(|e| e.reason.as_str()).collect()
    }

    #[test]
    fn warnings_come_first_errors_before_warnings_then_newest() {
        let events = [
            event(Status::Ok, "Scheduled", 1),
            event(Status::Warn, "Unhealthy", 11),
            event(Status::Err, "BackOff", 2),
            event(Status::Warn, "FailedMount", 6),
        ];
        assert_eq!(
            reasons(&warnings_first(&events)),
            ["BackOff", "FailedMount", "Unhealthy", "Scheduled"]
        );
    }

    #[test]
    fn recent_warnings_sort_by_age_and_cap() {
        let events = [
            event(Status::Warn, "Old", 42),
            event(Status::Ok, "Pulled", 1),
            event(Status::Err, "New", 2),
            event(Status::Warn, "Mid", 6),
        ];
        assert_eq!(reasons(&recent_warnings(&events, 2)), ["New", "Mid"]);
    }

    #[test]
    fn filter_by_type_and_text() {
        let events = [
            event(Status::Ok, "Scheduled", 1),
            event(Status::Err, "BackOff", 2),
            event(Status::Warn, "FailedMount", 6),
        ];
        assert_eq!(
            reasons(&filter_events(&events, EventFilter::Warnings, "")),
            ["BackOff", "FailedMount"]
        );
        assert_eq!(
            reasons(&filter_events(&events, EventFilter::Normal, "")),
            ["Scheduled"]
        );
        assert_eq!(
            reasons(&filter_events(&events, EventFilter::All, "mount")),
            ["FailedMount"]
        );
        assert_eq!(
            reasons(&filter_events(&events, EventFilter::Normal, "backoff")),
            [""; 0]
        );
    }
}
