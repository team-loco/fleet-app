use std::time::Duration;

use crate::filter::TextFilter;
use crate::overview::{join_clauses, plural};
use crate::time::age;
use crate::{ClusterId, Counts, ResourceRef, Status};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub details: EventDetails,
    pub status: Status,
    pub reason: String,
    pub object: String,
    pub namespace: Option<String>,
    pub message: String,
    pub age: Duration,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventDetails {
    pub uid: String,
    pub kind: String,
    pub object_uid: Option<String>,
    pub object_namespace: Option<String>,
    pub event_type: String,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub first_age: Option<Duration>,
    pub source_component: String,
    pub source_host: String,
}

impl Event {
    pub fn target(&self, cluster: &ClusterId) -> Option<ResourceRef> {
        let (kind, name) = self.object.split_once('/')?;
        if name.is_empty() || kind.is_empty() {
            return None;
        }
        let kind = if self.details.kind.is_empty() {
            crate::resource::KINDS
                .iter()
                .find(|info| info.kind.eq_ignore_ascii_case(kind))
                .map_or(kind, |info| info.kind)
        } else {
            &self.details.kind
        };
        let namespace = match kind {
            "Node" => None,
            "Namespace" => None,
            _ => self
                .details
                .object_namespace
                .clone()
                .or_else(|| self.namespace.clone()),
        };
        Some(ResourceRef {
            cluster: cluster.clone(),
            kind: kind.into(),
            namespace,
            name: name.into(),
        })
    }

    pub fn same_object(&self, other: &Self) -> bool {
        match (&self.details.object_uid, &other.details.object_uid) {
            (Some(left), Some(right)) => left == right,
            _ => self.object == other.object && self.namespace == other.namespace,
        }
    }

    pub fn key(&self) -> String {
        if !self.details.uid.is_empty() {
            return self.details.uid.clone();
        }
        format!(
            "{:?}/{}/{}/{}",
            self.namespace, self.object, self.reason, self.message
        )
    }

    pub fn is_warning(&self) -> bool {
        self.status != Status::Ok
    }

    fn matches(&self, filter: EventFilter, query: &TextFilter) -> bool {
        let fields = [
            self.reason.as_str(),
            self.object.as_str(),
            self.message.as_str(),
            self.namespace.as_deref().unwrap_or_default(),
        ];
        filter.keeps(self) && query.matches(self.namespace.as_deref(), &fields)
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

    pub fn keeps(self, event: &Event) -> bool {
        match self {
            Self::All => true,
            Self::Warnings => event.is_warning(),
            Self::Normal => !event.is_warning(),
        }
    }
}

pub fn filter_counts(events: &[Event], query: &TextFilter) -> Vec<(EventFilter, usize)> {
    let matching: Vec<&Event> = events
        .iter()
        .filter(|e| e.matches(EventFilter::All, query))
        .collect();
    EventFilter::ALL
        .iter()
        .map(|filter| (*filter, matching.iter().filter(|e| filter.keeps(e)).count()))
        .collect()
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

pub fn counts(events: &[Event]) -> Counts {
    let flagged = Counts::of(events.iter().map(|e| e.status));
    Counts {
        total: None,
        ..flagged
    }
}

pub fn sentence(events: &[Event]) -> String {
    let counts = counts(events);
    let newest = events
        .iter()
        .filter(|e| e.is_warning())
        .min_by_key(|e| (e.age, std::cmp::Reverse(e.status)));
    let Some(newest) = newest else {
        return if events.is_empty() {
            "No events have been recorded recently.".into()
        } else {
            format!(
                "No warnings; all {} are Normal.",
                plural(events.len(), "event", "events")
            )
        };
    };
    let mut tally = Vec::new();
    if counts.err > 0 {
        tally.push(plural(counts.err as usize, "error", "errors"));
    }
    if counts.warn > 0 {
        tally.push(plural(counts.warn as usize, "warning", "warnings"));
    }
    format!(
        "{}; the newest is {} on {}, {} ago.",
        join_clauses(&tally),
        newest.reason,
        newest.object,
        age(newest.age)
    )
}

pub fn in_namespace(events: &[Event], namespace: Option<&str>) -> Vec<Event> {
    events
        .iter()
        .filter(|event| {
            namespace.is_none_or(|namespace| event.namespace.as_deref() == Some(namespace))
        })
        .cloned()
        .collect()
}

pub fn filter_events(events: &[Event], filter: EventFilter, query: &TextFilter) -> Vec<Event> {
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
            details: EventDetails::default(),
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
    fn namespace_filter_keeps_all_explicit_and_empty_namespaces_empty() {
        let mut rows = vec![event(Status::Warn, "BackOff", 1)];
        let mut other = rows[0].clone();
        other.namespace = Some("default".into());
        rows.push(other);
        assert_eq!(in_namespace(&rows, None).len(), 2);
        assert_eq!(in_namespace(&rows, Some("payments")).len(), 1);
        assert_eq!(in_namespace(&rows, Some("missing")).len(), 0);
    }

    #[test]
    fn object_links_use_canonical_kinds_and_preserve_namespace() {
        let mut row = event(Status::Warn, "BackOff", 1);
        let cluster = ClusterId::new("dev");
        let target = row.target(&cluster).unwrap_or_else(|| panic!("target"));
        assert_eq!(target.kind, "Pod");
        assert_eq!(target.namespace.as_deref(), Some("payments"));
        assert_eq!(target.name, "backoff");
        row.object = "node/worker".into();
        assert_eq!(
            row.target(&cluster).and_then(|target| target.namespace),
            None
        );
        row.object = "pod/".into();
        assert!(row.target(&cluster).is_none());
    }

    #[test]
    fn object_identity_distinguishes_namespaces_and_recreated_objects() {
        let row = event(Status::Warn, "BackOff", 1);
        let mut other = row.clone();
        other.reason = "Pulled".into();
        assert!(row.same_object(&other));
        other.namespace = Some("default".into());
        assert!(!row.same_object(&other));
        let mut row = row;
        row.details.object_uid = Some("old".into());
        other = row.clone();
        other.details.object_uid = Some("new".into());
        assert!(!row.same_object(&other));
    }

    #[test]
    fn stable_uid_keeps_selection_when_message_and_count_change() {
        let mut row = event(Status::Warn, "BackOff", 1);
        row.details.uid = "event-uid".into();
        let mut updated = row.clone();
        updated.count += 1;
        updated.message = "retrying".into();
        assert_eq!(row.key(), updated.key());
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
    fn counts_split_errors_from_warnings() {
        let events = [
            event(Status::Ok, "Scheduled", 1),
            event(Status::Err, "BackOff", 2),
            event(Status::Warn, "FailedMount", 6),
            event(Status::Warn, "Unhealthy", 11),
        ];
        assert_eq!(counts(&events), Counts::default().with_err(1).with_warn(2));
    }

    #[test]
    fn sentence_counts_then_names_the_newest_warning() {
        let events = [
            event(Status::Ok, "Scheduled", 1),
            event(Status::Warn, "FailedMount", 6),
            event(Status::Err, "BackOff", 2),
            event(Status::Warn, "Unhealthy", 11),
        ];
        assert_eq!(
            sentence(&events),
            "1 error and 2 warnings; the newest is BackOff on pod/backoff, 2m ago."
        );
        assert_eq!(
            sentence(&events[1..2]),
            "1 warning; the newest is FailedMount on pod/failedmount, 6m ago."
        );
    }

    #[test]
    fn sentence_without_warnings() {
        let events = [
            event(Status::Ok, "Scheduled", 1),
            event(Status::Ok, "Pulled", 1),
        ];
        assert_eq!(sentence(&events), "No warnings; all 2 events are Normal.");
        assert_eq!(sentence(&[]), "No events have been recorded recently.");
    }

    #[test]
    fn filter_by_type_and_text() {
        let events = [
            event(Status::Ok, "Scheduled", 1),
            event(Status::Err, "BackOff", 2),
            event(Status::Warn, "FailedMount", 6),
        ];
        assert_eq!(
            reasons(&filter_events(
                &events,
                EventFilter::Warnings,
                &TextFilter::default()
            )),
            ["BackOff", "FailedMount"]
        );
        assert_eq!(
            reasons(&filter_events(
                &events,
                EventFilter::Normal,
                &TextFilter::default()
            )),
            ["Scheduled"]
        );
        assert_eq!(
            reasons(&filter_events(
                &events,
                EventFilter::All,
                &TextFilter::parse("mount")
            )),
            ["FailedMount"]
        );
        assert_eq!(
            reasons(&filter_events(
                &events,
                EventFilter::Normal,
                &TextFilter::parse("backoff")
            )),
            [""; 0]
        );
    }
}
