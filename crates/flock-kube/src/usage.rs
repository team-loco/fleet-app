use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use flock_core::overview::{TimeRange, Utilization};
use jiff::{SignedDuration, Timestamp};
use k8s_openapi::api::core::v1::Node;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

use crate::error::Result;
use crate::manager::ClusterHandle;
use crate::metrics::{NodeUsage, RangeQuery, Series, Usage, cpu_millicores, parse_quantity};
use crate::probe::PrometheusTarget;

const PERCENT: f64 = 100.;
const MILLI_PER_CORE: f64 = 1e3;
const BYTES_PER_GIB: f64 = 1_073_741_824.;
const CPU: &str = "cpu";
const MEMORY: &str = "memory";
const CHART_POINTS: u32 = 13;
const HOUR_SECS: u64 = 3600;
const CPU_QUERY: &str = "100 * (1 - avg(rate(node_cpu_seconds_total{mode=\"idle\"}[5m])))";
const MEMORY_QUERY: &str =
    "100 * (1 - sum(node_memory_MemAvailable_bytes) / sum(node_memory_MemTotal_bytes))";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Allocatable {
    pub cpu_millicores: f64,
    pub memory_bytes: f64,
}

fn quantity<'a>(
    map: Option<&'a std::collections::BTreeMap<String, Quantity>>,
    key: &str,
) -> Option<&'a str> {
    map?.get(key).map(|q| q.0.as_str())
}

pub fn allocatable(node: &Node) -> Allocatable {
    let map = node.status.as_ref().and_then(|s| s.allocatable.as_ref());
    Allocatable {
        cpu_millicores: quantity(map, CPU)
            .and_then(cpu_millicores)
            .unwrap_or_default(),
        memory_bytes: quantity(map, MEMORY)
            .and_then(parse_quantity)
            .unwrap_or_default(),
    }
}

pub fn cores(millicores: f64) -> f32 {
    narrow(millicores / MILLI_PER_CORE)
}

pub fn gib(bytes: f64) -> f32 {
    narrow(bytes / BYTES_PER_GIB)
}

#[allow(clippy::cast_possible_truncation)]
pub fn narrow(value: f64) -> f32 {
    value as f32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub cpu: f32,
    pub memory: f32,
}

impl Sample {
    pub fn percents(&self) -> (u8, u8) {
        (whole_percent(self.cpu), whole_percent(self.memory))
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole_percent(value: f32) -> u8 {
    value.round().clamp(0., f32::from(u8::MAX)) as u8
}

pub fn sample(nodes: &[Arc<Node>], usage: &[NodeUsage]) -> Option<Sample> {
    let used: HashMap<&str, &Usage> = usage.iter().map(|u| (u.name.as_str(), &u.usage)).collect();
    let mut total = Allocatable::default();
    let mut busy = Allocatable::default();
    for node in nodes {
        let name = node.metadata.name.as_deref().unwrap_or_default();
        let Some(usage) = used.get(name) else {
            continue;
        };
        let capacity = allocatable(node);
        total.cpu_millicores += capacity.cpu_millicores;
        total.memory_bytes += capacity.memory_bytes;
        busy.cpu_millicores += usage.cpu_millicores;
        busy.memory_bytes += usage.memory_bytes;
    }
    if total.cpu_millicores <= 0. || total.memory_bytes <= 0. {
        return None;
    }
    Some(Sample {
        cpu: narrow(PERCENT * busy.cpu_millicores / total.cpu_millicores),
        memory: narrow(PERCENT * busy.memory_bytes / total.memory_bytes),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct UsageRing {
    samples: VecDeque<Sample>,
    capacity: usize,
}

impl UsageRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, sample: Sample) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn latest(&self) -> Option<Sample> {
        self.samples.back().copied()
    }

    pub fn utilization(&self, cpu_requests: f32) -> Option<Utilization> {
        if self.samples.is_empty() {
            return None;
        }
        Some(Utilization {
            range: TimeRange::Hour,
            cpu: self.samples.iter().map(|s| s.cpu).collect(),
            memory: self.samples.iter().map(|s| s.memory).collect(),
            cpu_requests,
        })
    }
}

pub fn window(range: TimeRange) -> Duration {
    let hours = match range {
        TimeRange::Hour => 1,
        TimeRange::SixHours => 6,
        TimeRange::Day => 24,
        TimeRange::Week => 24 * 7,
    };
    Duration::from_secs(hours * HOUR_SECS)
}

pub fn range_query(query: &str, range: TimeRange, now: Timestamp) -> RangeQuery {
    let span = window(range);
    let step = span.as_secs() / u64::from(CHART_POINTS - 1);
    let back = SignedDuration::try_from(span).unwrap_or_default();
    RangeQuery {
        query: query.to_owned(),
        start: now.checked_sub(back).unwrap_or(now),
        end: now,
        step_seconds: u32::try_from(step).unwrap_or(u32::MAX),
    }
}

pub fn values(series: &[Series]) -> Vec<f32> {
    series
        .first()
        .map(|s| s.points.iter().map(|(_, v)| narrow(*v)).collect())
        .unwrap_or_default()
}

async fn utilization(
    handle: &ClusterHandle,
    target: &PrometheusTarget,
    range: TimeRange,
    now: Timestamp,
) -> Result<Utilization> {
    let cpu_query = range_query(CPU_QUERY, range, now);
    let memory_query = range_query(MEMORY_QUERY, range, now);
    let (cpu, memory) = futures::try_join!(
        handle.prometheus_range(target, &cpu_query),
        handle.prometheus_range(target, &memory_query),
    )?;
    Ok(Utilization {
        range,
        cpu: values(&cpu),
        memory: values(&memory),
        cpu_requests: 0.,
    })
}

pub async fn prometheus_utilization(
    handle: &ClusterHandle,
    target: &PrometheusTarget,
    now: Timestamp,
) -> Result<Vec<Utilization>> {
    let ranges = TimeRange::ALL.map(|range| utilization(handle, target, range, now));
    futures::future::try_join_all(ranges).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::problems::tests::parse;

    const ROUND_TRIP: Duration = Duration::from_secs(1);
    const EMPTY_MATRIX: &str = r#"{"status":"success","data":{"resultType":"matrix","result":[]}}"#;

    #[tokio::test(start_paused = true)]
    async fn every_range_query_runs_at_once() {
        let (client, traffic) = crate::fake::client(ROUND_TRIP, |_, path| {
            path.contains("/proxy/")
                .then(|| (http::StatusCode::OK, EMPTY_MATRIX.to_owned()))
        });
        let (_manager, handle) = crate::fake::handle(&client);
        let target = PrometheusTarget {
            namespace: "monitoring".to_owned(),
            service: "prometheus".to_owned(),
            port: 9090,
            scheme: "http".to_owned(),
        };
        let started = tokio::time::Instant::now();
        let ranges = prometheus_utilization(&handle, &target, Timestamp::UNIX_EPOCH).await;
        assert!(started.elapsed() < ROUND_TRIP * 2);
        assert_eq!(ranges.map(|r| r.len()).ok(), Some(TimeRange::ALL.len()));
        assert!(traffic.peak() >= TimeRange::ALL.len() * 2);
    }

    fn node(name: &str, cpu: &str, memory: &str) -> Arc<Node> {
        parse(json!({
            "metadata": { "name": name },
            "status": { "allocatable": { "cpu": cpu, "memory": memory } }
        }))
    }

    fn used(name: &str, millicores: f64, gib: f64) -> NodeUsage {
        NodeUsage {
            name: name.into(),
            usage: Usage {
                cpu_millicores: millicores,
                memory_bytes: gib * BYTES_PER_GIB,
            },
        }
    }

    #[test]
    fn sample_divides_usage_by_allocatable_of_reporting_nodes() {
        let nodes = [
            node("a", "4", "8Gi"),
            node("b", "4000m", "8Gi"),
            node("c", "4", "8Gi"),
        ];
        let usage = [used("a", 1000., 2.), used("b", 3000., 6.)];
        let Some(found) = sample(&nodes, &usage) else {
            panic!("sample");
        };
        assert_eq!(found.percents(), (50, 50));
        assert_eq!(sample(&nodes, &[]), None);
    }

    #[test]
    fn ring_keeps_the_newest_samples_in_order() {
        let mut ring = UsageRing::new(3);
        for cpu in [1., 2., 3., 4.] {
            ring.push(Sample {
                cpu,
                memory: cpu * 10.,
            });
        }
        let Some(hour) = ring.utilization(74.) else {
            panic!("utilization");
        };
        assert_eq!(hour.cpu, [2., 3., 4.]);
        assert_eq!(hour.memory, [20., 30., 40.]);
        assert_eq!(hour.range, TimeRange::Hour);
        assert_eq!(ring.latest().map(|s| s.cpu), Some(4.));
        assert_eq!(UsageRing::new(3).utilization(0.), None);
    }

    #[test]
    fn range_queries_span_the_window_in_twelve_steps() {
        let now = Timestamp::from_second(1_800_000_000).unwrap_or_default();
        let query = range_query(CPU_QUERY, TimeRange::Day, now);
        assert_eq!(query.end.as_second() - query.start.as_second(), 86_400);
        assert_eq!(query.step_seconds, 7_200);
    }
}
