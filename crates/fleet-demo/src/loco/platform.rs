use std::time::Duration;

use fleet_core::Status;
use fleet_core::loco::{AgentLevel, AgentLogLine, AgentSync, Component, Platform, Reconnects};

const NAMESPACE: &str = "loco-system";
const IMAGES: &str = "sha-8c1f2e7";
const CONTROL_PLANE: &str = "api.loco.build";
const UPTIME: Duration = Duration::from_mins(192);
const LAST_HEARTBEAT: Duration = Duration::from_secs(4);

pub fn platform() -> Platform {
    Platform {
        namespace: NAMESPACE.into(),
        images: Some(IMAGES.into()),
        agent: AgentSync {
            control_plane: Some(CONTROL_PLANE.into()),
            connected: Some(true),
            uptime: Some(UPTIME),
            last_heartbeat: Some(LAST_HEARTBEAT),
            kubernetes_healthy: Some(true),
            controller_healthy: Some(true),
            reconnects: Some(Reconnects {
                count: 2,
                last_at: Some("18:04".into()),
                reason: Some("stream closed by server (GOAWAY)".into()),
            }),
            builds_enabled: Some(true),
        },
        components: components(),
        log: log(),
    }
}

fn component(
    status: Status,
    name: &str,
    namespace: &str,
    ready: (u32, u32),
    version: &str,
    detail: &str,
) -> Component {
    Component {
        status,
        name: name.into(),
        namespace: namespace.into(),
        ready: Some(ready),
        version: Some(version.into()),
        detail: detail.into(),
    }
}

fn components() -> Vec<Component> {
    let ok = Status::Ok;
    vec![
        component(
            ok,
            "loco-agent",
            NAMESPACE,
            (1, 1),
            IMAGES,
            "sync stream up 3h 12m",
        ),
        component(
            ok,
            "loco-controller",
            NAMESPACE,
            (1, 1),
            IMAGES,
            "application reconciler · 214 Applications",
        ),
        component(
            ok,
            "loco-build-controller",
            NAMESPACE,
            (1, 1),
            IMAGES,
            "build reconciler · jobs in loco-builds",
        ),
        component(
            ok,
            "envoy-gateway",
            "envoy-gateway-system",
            (1, 1),
            "v1.5.1",
            "GatewayClass eg accepted",
        ),
        component(
            Status::Warn,
            "envoy-loco-system-eg",
            "envoy-gateway-system",
            (3, 3),
            "v1.35.2",
            "2 routes with problems",
        ),
        component(
            ok,
            "cert-manager",
            "cert-manager",
            (1, 1),
            "v1.18.2",
            "wildcard *.onloco.app renews in 61d",
        ),
        component(
            ok,
            "cilium",
            "kube-system",
            (14, 14),
            "v1.18.1",
            "workspace network policies enforced",
        ),
        component(
            ok,
            "otel-collector",
            "observability-node",
            (14, 14),
            "0.131.0",
            "logs, metrics, traces",
        ),
        component(
            ok,
            "clickhouse",
            "observability",
            (1, 1),
            "25.6",
            "18 GiB · 7d retention",
        ),
        component(
            ok,
            "observability-proxy",
            "observability",
            (1, 1),
            IMAGES,
            "",
        ),
    ]
}

fn line(time: &str, level: AgentLevel, message: &str) -> AgentLogLine {
    AgentLogLine {
        time: time.into(),
        level,
        message: message.into(),
    }
}

fn log() -> Vec<AgentLogLine> {
    vec![
        line(
            "21:14:02",
            AgentLevel::Info,
            "placement applied  resource=checkout  revision=14",
        ),
        line(
            "21:13:58",
            AgentLevel::Info,
            "heartbeat  pods=412/1540  cpu=68.4/112  controller=ok",
        ),
        line(
            "21:13:41",
            AgentLevel::Error,
            "apply failed  resource=api  revision=22  retrying=true  err=Application.infra.loco.io is invalid",
        ),
        line(
            "21:13:12",
            AgentLevel::Info,
            "build created  build=0192f3a1  resource=frontend",
        ),
        line(
            "21:12:30",
            AgentLevel::Warn,
            "env secret not staged  resource=frontend  revision=4",
        ),
        line(
            "21:11:58",
            AgentLevel::Info,
            "inventory sent  placements=214  builds=3",
        ),
    ]
}
