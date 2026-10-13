use std::collections::BTreeMap;

use fleet_core::{ClusterId, ResourceRef, ResourceSummary, Status};

const PROD_US: &str = "prod-us-east";
const PROD_EU: &str = "prod-eu-west";
const STAGING: &str = "staging";
const DEV: &str = "dev-kind";
const PAYMENTS: &str = "payments";
const FLUX_SYSTEM: &str = "flux-system";
const CHECKOUT: &str = "checkout-api";
const APPS_PAYMENTS: &str = "apps-payments";
const APP_LABEL: &str = "app";
const HTTP_PORT: u16 = 80;

fn reference(cluster: &str, kind: &str, namespace: Option<&str>, name: &str) -> ResourceRef {
    ResourceRef {
        cluster: ClusterId::new(cluster),
        kind: kind.into(),
        namespace: namespace.map(Into::into),
        name: name.into(),
    }
}

fn entry(resource: ResourceRef, status: Option<Status>, detail: &str) -> ResourceSummary {
    ResourceSummary {
        resource,
        status,
        detail: detail.into(),
        labels: BTreeMap::new(),
        ports: Vec::new(),
        owner: None,
    }
}

fn checkout(
    cluster: &str,
    kind: &str,
    name: &str,
    status: Option<Status>,
    detail: &str,
) -> ResourceSummary {
    let resource = reference(cluster, kind, Some(PAYMENTS), name);
    let mut summary = entry(resource, status, detail);
    summary.labels.insert(APP_LABEL.into(), CHECKOUT.into());
    summary
}

fn checkout_deployment(cluster: &str, status: Status, detail: &str) -> ResourceSummary {
    let mut summary = checkout(cluster, "Deployment", CHECKOUT, Some(status), detail);
    summary.owner = Some(reference(
        cluster,
        "Kustomization",
        Some(FLUX_SYSTEM),
        APPS_PAYMENTS,
    ));
    summary
}

pub fn resources() -> Vec<ResourceSummary> {
    let mut service = checkout(PROD_US, "Service", CHECKOUT, None, "ClusterIP :80");
    service.ports = vec![HTTP_PORT];
    vec![
        checkout_deployment(PROD_US, Status::Err, "2/3 ready"),
        checkout(
            PROD_US,
            "Pod",
            "checkout-api-7f9c6d-x2lqp",
            Some(Status::Err),
            "CrashLoopBackOff",
        ),
        checkout_deployment(PROD_EU, Status::Ok, "3/3 · 2.13.4"),
        checkout_deployment(STAGING, Status::Ok, "2/2"),
        checkout_deployment(DEV, Status::Ok, "1/1"),
        service,
        checkout(
            PROD_US,
            "Secret",
            "checkout-api-env",
            None,
            "Opaque · 4 keys",
        ),
        entry(
            reference(PROD_US, "Kustomization", Some(FLUX_SYSTEM), APPS_PAYMENTS),
            Some(Status::Err),
            "Health check failed",
        ),
        entry(
            reference(PROD_US, "Pod", Some("ml"), "ml-batch-5c7f-9kq2d"),
            Some(Status::Warn),
            "FailedScheduling",
        ),
        entry(
            reference(PROD_US, "Pod", Some("monitoring"), "grafana-6b8d9-tw4mn"),
            Some(Status::Warn),
            "FailedMount",
        ),
        entry(
            reference(PROD_US, "Node", None, "ip-10-4-31-12"),
            Some(Status::Warn),
            "Cordoned",
        ),
        entry(
            reference(PROD_EU, "Kustomization", Some(FLUX_SYSTEM), APPS_PAYMENTS),
            Some(Status::Suspended),
            "Suspended",
        ),
        entry(
            reference(
                PROD_EU,
                "HorizontalPodAutoscaler",
                Some("web"),
                "web-frontend",
            ),
            Some(Status::Warn),
            "At max replicas (12/12)",
        ),
        entry(
            reference(
                STAGING,
                "HelmRelease",
                Some("monitoring"),
                "kube-prometheus-stack",
            ),
            Some(Status::Warn),
            "Upgrade retrying (2/3)",
        ),
        entry(
            reference(STAGING, "Pod", Some("search"), "search-indexer-0"),
            Some(Status::Warn),
            "Readiness probe failed",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use fleet_core::palette::{Section, search};

    use crate::snapshot;

    fn single(range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
        vec![range]
    }

    #[test]
    fn checkout_query_reproduces_the_design() {
        let results = search(&snapshot(), None, "checkout");
        assert_eq!(results.summary(), "11 matches · 6 clusters");
        let rows: Vec<String> = results
            .hits
            .iter()
            .map(|hit| match hit.section {
                Section::Commands => format!("> {} {}", hit.name, hit.shortcut.unwrap_or("")),
                _ => format!(
                    "{} {} {} {} {}",
                    hit.kind, hit.name, hit.namespace, hit.cluster, hit.detail
                ),
            })
            .collect();
        assert_eq!(
            rows,
            [
                "deploy checkout-api payments prod-us-east 2/3 ready",
                "pod checkout-api-7f9c6d-x2lqp payments prod-us-east CrashLoopBackOff",
                "deploy checkout-api payments prod-eu-west 3/3 · 2.13.4",
                "deploy checkout-api payments staging 2/2",
                "deploy checkout-api payments dev-kind 1/1",
                "svc checkout-api payments prod-us-east ClusterIP :80",
                "secret checkout-api-env payments prod-us-east Opaque · 4 keys",
                "> Logs: follow all checkout-api pods on prod-us-east ⌘L",
                "> Compare deploy/checkout-api across clusters ⌘⇧D",
                "> Show Flux owner of deploy/checkout-api ⌘⇧G",
                "> Port-forward svc/checkout-api 8080 → 80 ",
            ]
        );
        let first = results.hits.first().map(|hit| hit.highlights.clone());
        assert_eq!(first, Some(single(0..8)));
    }
}
