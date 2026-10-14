use std::collections::BTreeMap;

use flock_core::Status;
use flock_core::loco::{Listener, Route, Routes};

use super::app_ref;

const GATEWAY: &str = "loco-system/eg";
const ENVOY_READY: (u32, u32) = (3, 3);
const ROUTES_TOTAL: usize = 44;
const APPS_HOSTS: &str = "*.onloco.app";
const PLATFORM_HOSTS: &str = "*.loco.build";
const HTTPS: u16 = 443;
const HTTP: u16 = 80;

pub fn routes() -> Routes {
    Routes {
        gateway: GATEWAY.into(),
        envoy_ready: Some(ENVOY_READY),
        http3: true,
        listeners: listeners(),
        total: ROUTES_TOTAL,
        routes: http_routes(),
    }
}

fn listener(
    name: &str,
    hostname: &str,
    port: u16,
    http3: bool,
    routes: u32,
    tls: &str,
) -> Listener {
    Listener {
        status: Status::Ok,
        name: name.into(),
        hostname: Some(hostname.into()),
        port,
        http3,
        routes: Some(routes),
        tls: tls.into(),
    }
}

fn listeners() -> Vec<Listener> {
    vec![
        listener(
            "https-apps-0",
            APPS_HOSTS,
            HTTPS,
            true,
            41,
            "loco-core-tls · Let’s Encrypt · renews in 61d",
        ),
        listener(
            "http-apps-0",
            APPS_HOSTS,
            HTTP,
            false,
            41,
            "redirect → https",
        ),
        listener(
            "https-platform",
            PLATFORM_HOSTS,
            HTTPS,
            true,
            3,
            "loco-core-tls · renews in 61d",
        ),
    ]
}

struct RouteSeed {
    status: Status,
    host: Option<&'static str>,
    name: &'static str,
    backend: &'static str,
    ready: Option<(u32, u32)>,
    detail: &'static str,
    app: Option<&'static str>,
}

fn http_routes() -> Vec<Route> {
    let seeds = [
        RouteSeed {
            status: Status::Warn,
            host: Some("checkout.onloco.app"),
            name: "checkout",
            backend: "ws-0192c3e7/checkout:8080",
            ready: Some((0, 2)),
            detail: "Accepted · no ready endpoints (app failing) → 503",
            app: Some("checkout"),
        },
        RouteSeed {
            status: Status::Warn,
            host: Some("old-landing.onloco.app"),
            name: "old-landing",
            backend: "ws-0192c3f1/old-landing:8080",
            ready: None,
            detail: "ResolvedRefs=False · BackendNotFound",
            app: None,
        },
        RouteSeed {
            status: Status::Ok,
            host: Some("auth.onloco.app"),
            name: "auth",
            backend: "ws-0192c3e7/auth:8080",
            ready: Some((3, 3)),
            detail: "Accepted",
            app: Some("auth"),
        },
        RouteSeed {
            status: Status::Ok,
            host: None,
            name: "checkout-worker",
            backend: "ws-0192c3e7/checkout-worker:8080",
            ready: Some((2, 2)),
            detail: "private · no hostname",
            app: Some("checkout-worker"),
        },
        RouteSeed {
            status: Status::Ok,
            host: Some("frontend.onloco.app"),
            name: "frontend",
            backend: "ws-0192c3f1/frontend:3000",
            ready: Some((4, 4)),
            detail: "Accepted",
            app: Some("frontend"),
        },
        RouteSeed {
            status: Status::Ok,
            host: Some("frontend-staging.onloco.app"),
            name: "frontend-staging",
            backend: "ws-0192c3f1/frontend-staging:3000",
            ready: Some((1, 2)),
            detail: "Accepted · rolling out",
            app: Some("frontend-stg"),
        },
        RouteSeed {
            status: Status::Ok,
            host: Some("api.beta.dev"),
            name: "api",
            backend: "ws-0192d0aa/api:8080",
            ready: Some((2, 2)),
            detail: "Accepted · custom domain",
            app: Some("api"),
        },
        RouteSeed {
            status: Status::Ok,
            host: Some("obs.loco.build"),
            name: "observability-proxy",
            backend: "observability/observability-proxy:8080",
            ready: Some((1, 1)),
            detail: "Accepted · platform listener",
            app: None,
        },
    ];
    seeds
        .into_iter()
        .map(|seed| Route {
            labels: BTreeMap::new(),
            status: seed.status,
            name: seed.name.into(),
            host: seed.host.map(Into::into),
            backend: seed.backend.into(),
            ready: seed.ready,
            detail: seed.detail.into(),
            application: seed.app.map(app_ref),
        })
        .collect()
}
