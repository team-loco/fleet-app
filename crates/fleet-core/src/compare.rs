use crate::changes::{LineChange, PatchLine};
use crate::{ClusterId, ResourceRef, Status};

const DASH: char = '-';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
    pub clusters: Vec<ComparedCluster>,
    pub fields: Vec<Field>,
    pub spec: Option<SpecDiff>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparedCluster {
    pub cluster: ClusterId,
    pub status: Status,
    pub health: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub label: String,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub flag: Option<Status>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecDiff {
    pub path: String,
    pub base: ClusterId,
    pub other: ClusterId,
    pub lines: Vec<PatchLine>,
}

impl Cell {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            flag: None,
        }
    }

    pub fn flagged(text: impl Into<String>, flag: Status) -> Self {
        Self {
            text: text.into(),
            flag: Some(flag),
        }
    }
}

impl Field {
    pub fn differs(&self) -> bool {
        let first = self.cells.first().map(|c| c.text.as_str());
        self.cells.iter().any(|c| Some(c.text.as_str()) != first)
    }
}

impl SpecDiff {
    pub fn has_changes(&self) -> bool {
        self.lines.iter().any(|l| l.change != LineChange::Context)
    }

    pub fn apply_label(&self) -> String {
        let (base, other) = short_names(self.base.as_str(), self.other.as_str());
        format!("Apply {other} value to {base}…")
    }
}

impl Comparison {
    pub fn matches(&self, resource: &ResourceRef) -> bool {
        self.kind == resource.kind
            && self.namespace == resource.namespace
            && self.name == resource.name
    }

    pub fn on(&self, cluster: &ClusterId) -> ResourceRef {
        ResourceRef {
            cluster: cluster.clone(),
            kind: self.kind.clone(),
            namespace: self.namespace.clone(),
            name: self.name.clone(),
        }
    }

    pub fn visible(&self, differences_only: bool) -> Vec<&Field> {
        self.fields
            .iter()
            .filter(|f| !differences_only || f.differs())
            .collect()
    }

    pub fn differing(&self) -> usize {
        self.fields.iter().filter(|f| f.differs()).count()
    }

    pub fn hidden_note(&self, differences_only: bool) -> String {
        let hidden = self.fields.len() - self.visible(differences_only).len();
        match hidden {
            0 => "Showing all fields".into(),
            1 => "1 identical field hidden".into(),
            n => format!("{n} identical fields hidden"),
        }
    }

    pub fn sentence(&self) -> String {
        let count = self.clusters.len();
        let differing = self.differing();
        let total = self.fields.len();
        let broken = self
            .clusters
            .iter()
            .find(|c| c.status == Status::Err || c.status == Status::Warn);
        let summary = match differing {
            0 => format!("Same object on {count} clusters, and every field matches."),
            _ => format!("Same object on {count} clusters; {differing} of {total} fields differ."),
        };
        match broken {
            Some(c) => format!("{summary} {} is {}.", c.cluster, lowercase_first(&c.health)),
            None => summary,
        }
    }
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

pub fn short_names<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    let shared = a
        .split_inclusive(DASH)
        .zip(b.split_inclusive(DASH))
        .take_while(|(x, y)| x == y && x.ends_with(DASH))
        .map(|(x, _)| x.len())
        .sum::<usize>();
    let short_a = a.get(shared..).filter(|s| !s.is_empty()).unwrap_or(a);
    let short_b = b.get(shared..).filter(|s| !s.is_empty()).unwrap_or(b);
    (short_a, short_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(label: &str, values: &[&str]) -> Field {
        Field {
            label: label.into(),
            cells: values.iter().map(|v| Cell::new(*v)).collect(),
        }
    }

    fn compared(name: &str, status: Status, health: &str) -> ComparedCluster {
        ComparedCluster {
            cluster: ClusterId::new(name),
            status,
            health: health.into(),
        }
    }

    fn sample() -> Comparison {
        Comparison {
            kind: "Deployment".into(),
            namespace: Some("payments".into()),
            name: "checkout-api".into(),
            clusters: vec![
                compared("a", Status::Err, "Degraded · 1 pod crashlooping"),
                compared("b", Status::Ok, "Healthy"),
            ],
            fields: vec![
                field("Ready", &["2 / 3", "3 / 3"]),
                field("Memory", &["512Mi", "512Mi"]),
                field("Strategy", &["RollingUpdate", "RollingUpdate"]),
            ],
            spec: None,
        }
    }

    #[test]
    fn a_field_differs_when_any_cell_does() {
        assert!(field("x", &["1", "1", "2"]).differs());
        assert!(!field("x", &["1", "1"]).differs());
        assert!(!field("x", &[]).differs());
    }

    #[test]
    fn differences_only_hides_identical_fields() {
        let comparison = sample();
        let labels: Vec<&str> = comparison
            .visible(true)
            .iter()
            .map(|f| f.label.as_str())
            .collect();
        assert_eq!(labels, ["Ready"]);
        assert_eq!(comparison.visible(false).len(), 3);
        assert_eq!(comparison.hidden_note(true), "2 identical fields hidden");
        assert_eq!(comparison.hidden_note(false), "Showing all fields");
    }

    #[test]
    fn sentence_names_the_unhealthy_cluster() {
        assert_eq!(
            sample().sentence(),
            "Same object on 2 clusters; 1 of 3 fields differ. a is degraded · 1 pod crashlooping."
        );
        let mut healthy = sample();
        healthy.clusters[0] = compared("a", Status::Ok, "Healthy");
        healthy.fields.remove(0);
        assert_eq!(
            healthy.sentence(),
            "Same object on 2 clusters, and every field matches."
        );
    }

    #[test]
    fn comparisons_match_resources_on_any_cluster() {
        let comparison = sample();
        let elsewhere = comparison.on(&ClusterId::new("staging"));
        assert!(comparison.matches(&elsewhere));
        let other = ResourceRef {
            name: "cart".into(),
            ..elsewhere
        };
        assert!(!comparison.matches(&other));
    }

    #[test]
    fn short_names_drop_the_shared_prefix() {
        assert_eq!(
            short_names("prod-us-east", "prod-eu-west"),
            ("us-east", "eu-west")
        );
        assert_eq!(
            short_names("staging", "prod-us-east"),
            ("staging", "prod-us-east")
        );
        assert_eq!(short_names("prod-a", "prod-a"), ("a", "a"));
    }

    #[test]
    fn apply_label_moves_the_other_value_onto_the_base() {
        let diff = SpecDiff {
            path: "spec.template".into(),
            base: ClusterId::new("prod-us-east"),
            other: ClusterId::new("prod-eu-west"),
            lines: vec![PatchLine {
                change: LineChange::Context,
                number: 1,
                text: "containers:".into(),
                note: None,
            }],
        };
        assert_eq!(diff.apply_label(), "Apply eu-west value to us-east…");
        assert!(!diff.has_changes());
    }
}
