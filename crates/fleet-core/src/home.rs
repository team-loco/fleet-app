use std::collections::BTreeMap;
use std::time::Duration;

use crate::{ClusterId, Counts, ResourceRef, Status};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterMetrics {
    pub pods: Counts,
    pub cpu_percent: u8,
    pub memory_percent: u8,
    pub flux: Counts,
    pub revision: Revision,
}

impl ClusterMetrics {
    pub fn flux_ready(&self) -> u32 {
        let total = self.flux.total.unwrap_or_default();
        let not_ready = self.flux.err + self.flux.warn + self.flux.sus;
        total.saturating_sub(not_ready)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    pub sha: String,
    pub held: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub status: Status,
    pub target: ResourceRef,
    pub message: String,
    pub age: Duration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Severity {
    #[default]
    All,
    Errors,
    Warnings,
}

impl Severity {
    pub const ALL: [Self; 3] = [Self::All, Self::Errors, Self::Warnings];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Errors => "Errors",
            Self::Warnings => "Warnings",
        }
    }

    pub fn keeps(self, status: Status) -> bool {
        match self {
            Self::All => true,
            Self::Errors => status == Status::Err,
            Self::Warnings => status != Status::Err,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProblemGroup<'a> {
    pub cluster: &'a ClusterId,
    pub problems: Vec<&'a Problem>,
}

impl ProblemGroup<'_> {
    pub fn counts(&self) -> Counts {
        problem_counts(self.problems.iter().copied())
    }
}

pub fn problem_counts<'a>(problems: impl IntoIterator<Item = &'a Problem>) -> Counts {
    problems
        .into_iter()
        .fold(Counts::default(), |counts, problem| match problem.status {
            Status::Err => counts.with_err(counts.err + 1),
            Status::Warn => counts.with_warn(counts.warn + 1),
            Status::Suspended => counts.with_sus(counts.sus + 1),
            Status::Ok => counts,
            Status::Progress => counts,
            Status::Offline => counts,
        })
}

pub fn group_problems(problems: &[Problem], severity: Severity) -> Vec<ProblemGroup<'_>> {
    let mut groups: Vec<ProblemGroup<'_>> = Vec::new();
    let kept = problems.iter().filter(|p| severity.keeps(p.status));
    for problem in kept {
        let cluster = &problem.target.cluster;
        match groups.iter_mut().find(|g| g.cluster == cluster) {
            Some(group) => group.problems.push(problem),
            None => groups.push(ProblemGroup {
                cluster,
                problems: vec![problem],
            }),
        }
    }
    groups
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    Failing,
    Held,
    Retrying,
    NotTargeted,
}

impl Outcome {
    pub const fn status(self) -> Option<Status> {
        match self {
            Self::Applied => Some(Status::Ok),
            Self::Failing => Some(Status::Err),
            Self::Held => Some(Status::Suspended),
            Self::Retrying => Some(Status::Warn),
            Self::NotTargeted => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Failing => "failing",
            Self::Held => "held",
            Self::Retrying => "retrying",
            Self::NotTargeted => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lane {
    pub cluster: ClusterId,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub age: Duration,
    pub outcomes: BTreeMap<ClusterId, Outcome>,
}

impl Commit {
    pub fn outcome(&self, cluster: &ClusterId) -> Outcome {
        self.outcomes
            .get(cluster)
            .copied()
            .unwrap_or(Outcome::NotTargeted)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitOpsChanges {
    pub source: String,
    pub lanes: Vec<Lane>,
    pub commits: Vec<Commit>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FleetHome {
    pub problems: Vec<Problem>,
    pub changes: GitOpsChanges,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(cluster: &str, status: Status) -> Problem {
        Problem {
            status,
            target: ResourceRef {
                cluster: ClusterId::new(cluster),
                kind: "Pod".into(),
                namespace: Some("ns".into()),
                name: "p".into(),
            },
            message: String::new(),
            age: Duration::ZERO,
        }
    }

    fn sample() -> Vec<Problem> {
        vec![
            problem("a", Status::Err),
            problem("b", Status::Suspended),
            problem("a", Status::Warn),
            problem("c", Status::Err),
        ]
    }

    fn shape(groups: &[ProblemGroup<'_>]) -> Vec<(String, usize)> {
        groups
            .iter()
            .map(|g| (g.cluster.to_string(), g.problems.len()))
            .collect()
    }

    #[test]
    fn groups_keep_first_appearance_order() {
        let problems = sample();
        let groups = group_problems(&problems, Severity::All);
        assert_eq!(
            shape(&groups),
            [("a".into(), 2), ("b".into(), 1), ("c".into(), 1)]
        );
        assert_eq!(
            groups[0].counts(),
            Counts::default().with_err(1).with_warn(1)
        );
    }

    #[test]
    fn errors_filter_drops_groups_without_errors() {
        let problems = sample();
        let groups = group_problems(&problems, Severity::Errors);
        assert_eq!(shape(&groups), [("a".into(), 1), ("c".into(), 1)]);
    }

    #[test]
    fn warnings_filter_keeps_everything_but_errors() {
        let problems = sample();
        let groups = group_problems(&problems, Severity::Warnings);
        assert_eq!(shape(&groups), [("b".into(), 1), ("a".into(), 1)]);
        assert_eq!(groups[0].counts(), Counts::default().with_sus(1));
    }

    #[test]
    fn counts_tally_each_severity() {
        let counts = problem_counts(&sample());
        assert_eq!(
            counts,
            Counts::default().with_err(2).with_warn(1).with_sus(1)
        );
    }

    #[test]
    fn flux_ready_subtracts_flagged_objects() {
        let metrics = ClusterMetrics {
            pods: Counts::total(1),
            cpu_percent: 0,
            memory_percent: 0,
            flux: Counts::total(21).with_err(1),
            revision: Revision {
                sha: "a41e07c".into(),
                held: false,
            },
        };
        assert_eq!(metrics.flux_ready(), 20);
    }

    #[test]
    fn missing_lane_reads_as_not_targeted() {
        let commit = Commit {
            sha: String::new(),
            message: String::new(),
            author: String::new(),
            age: Duration::ZERO,
            outcomes: BTreeMap::from([(ClusterId::new("staging"), Outcome::Retrying)]),
        };
        assert_eq!(
            commit.outcome(&ClusterId::new("staging")),
            Outcome::Retrying
        );
        assert_eq!(commit.outcome(&ClusterId::new("dev")), Outcome::NotTargeted);
        assert_eq!(Outcome::NotTargeted.status(), None);
    }
}
