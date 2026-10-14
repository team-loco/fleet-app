use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClusterId(Arc<str>);

impl ClusterId {
    pub fn new(context: impl Into<String>) -> Self {
        let context = context.into();
        let shared = Arc::from(context);
        Self(shared)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for ClusterId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ClusterId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl fmt::Display for ClusterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub cluster: ClusterId,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(namespace) => write!(f, "{}/{}", namespace, self.name),
            None => f.write_str(&self.name),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uid(String);

impl Uid {
    pub fn new(uid: impl Into<String>) -> Self {
        Self(uid.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Seen {
    pub target: ResourceRef,
    pub uid: Uid,
}

impl Seen {
    pub fn new(target: ResourceRef, uid: Uid) -> Self {
        Self { target, uid }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_ref_displays_namespace_and_name() {
        let cluster = ClusterId::new("prod-us-east");
        let namespaced = ResourceRef {
            cluster: cluster.clone(),
            kind: "Deployment".into(),
            namespace: Some("payments".into()),
            name: "checkout-api".into(),
        };
        let node = ResourceRef {
            cluster,
            kind: "Node".into(),
            namespace: None,
            name: "ip-10-4-31-12".into(),
        };
        assert_eq!(namespaced.to_string(), "payments/checkout-api");
        assert_eq!(node.to_string(), "ip-10-4-31-12");
    }
}
