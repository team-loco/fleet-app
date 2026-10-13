use crate::{ResourceRef, Status};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub status: Status,
    pub title: String,
    pub detail: Option<String>,
}

impl Notice {
    pub fn failed(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            status: Status::Err,
            title: title.into(),
            detail: Some(detail.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forward {
    pub target: ResourceRef,
    pub local: u16,
    pub remote: u16,
}

impl Forward {
    pub fn label(&self) -> String {
        format!(
            "localhost:{} → {}/{}:{}",
            self.local,
            crate::resource::short_kind(&self.target.kind),
            self.target.name,
            self.remote
        )
    }
}

pub fn forwards_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 port-forward".to_owned()),
        n => Some(format!("{n} port-forwards")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClusterId;

    #[test]
    fn forward_label_names_the_service_and_ports() {
        let forward = Forward {
            target: ResourceRef {
                cluster: ClusterId::new("kind-fleet-a"),
                kind: "Service".into(),
                namespace: Some("apps".into()),
                name: "podinfo".into(),
            },
            local: 9898,
            remote: 9898,
        };
        assert_eq!(forward.label(), "localhost:9898 → svc/podinfo:9898");
    }

    #[test]
    fn status_bar_counts_forwards() {
        assert_eq!(forwards_label(0), None);
        assert_eq!(forwards_label(1).as_deref(), Some("1 port-forward"));
        assert_eq!(forwards_label(3).as_deref(), Some("3 port-forwards"));
    }
}
