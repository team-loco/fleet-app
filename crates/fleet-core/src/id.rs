use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClusterId(String);

impl ClusterId {
    pub fn new(context: impl Into<String>) -> Self {
        Self(context.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClusterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
