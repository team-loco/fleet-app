use std::hint::black_box;
use std::time::Duration;

use fleet_core::ClusterId;
use fleet_kube::detail::PodIndex;
use fleet_kube::settings::KubeSettings;
use fleet_kube::{lists, pods};
use jiff::Timestamp;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

#[path = "performance_support/scale.rs"]
mod scale;
#[path = "performance_support/synthetic.rs"]
mod synthetic;

const MAX_INDEX_ALLOCATIONS: u64 = 16;
const MAX_POD_ALLOCATIONS: u64 = 140_016;
const MAX_CONFIGMAP_ALLOCATIONS: u64 = 20_016;

const WATCHER_HEAP_BUDGET: usize = 32 * 1024 * 1024;
const CANCEL_WAIT: Duration = Duration::from_millis(100);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FLEET_E2E=1 and e2e:scale; run alone with --test-threads=1"]
async fn kind_scale_watcher_heap() {
    let (_directory, _sources, context) = scale::context();
    let profiler = dhat::Profiler::builder().testing().build();
    let settings = KubeSettings::default();
    let (manager, handle, subscriptions) = scale::populated_stores(&context, &settings).await;
    let populated = dhat::HeapStats::get().curr_bytes;
    drop(subscriptions);
    tokio::time::sleep(CANCEL_WAIT).await;
    let emptied = dhat::HeapStats::get().curr_bytes;
    let retained = populated.saturating_sub(emptied);
    println!(
        "watcher stores + watch tasks released heap={retained} bytes, populated process heap={populated} bytes"
    );
    assert!(retained > 0, "watcher memory was not released");
    assert!(
        retained < WATCHER_HEAP_BUDGET,
        "watcher heap exceeds budget: {retained}"
    );
    manager.disconnect(&context.id);
    drop(handle);
    drop(manager);
    drop(profiler);
}

#[test]
#[ignore = "run alone in release mode with --test-threads=1"]
fn synthetic_snapshot_allocations() {
    let raw = synthetic::pods();
    let maps = synthetic::configmaps();
    let cluster = ClusterId::new("scale");
    let now = Timestamp::now();
    let profiler = dhat::Profiler::builder().testing().build();
    let rows = pods::pods(&cluster, &raw, &[], &[], now);
    let pod_stats = dhat::HeapStats::get();
    let index = PodIndex::build(&raw, now);
    let overview_stats = dhat::HeapStats::get();
    println!(
        "10k pod rows allocated {} blocks, {} bytes; overview added {} blocks, {} bytes",
        pod_stats.total_blocks,
        pod_stats.total_bytes,
        overview_stats.total_blocks - pod_stats.total_blocks,
        overview_stats.total_bytes - pod_stats.total_bytes
    );
    assert!(
        pod_stats.total_blocks <= MAX_POD_ALLOCATIONS,
        "pod rows allocated {} blocks",
        pod_stats.total_blocks
    );
    let index_allocations = overview_stats.total_blocks - pod_stats.total_blocks;
    assert!(
        index_allocations <= MAX_INDEX_ALLOCATIONS,
        "overview allocated {index_allocations} blocks"
    );
    let configmaps: Vec<_> = maps.iter().map(|map| lists::config_map(map, now)).collect();
    let configmap_stats = dhat::HeapStats::get();
    let allocations = configmap_stats.total_blocks - overview_stats.total_blocks;
    println!("5k configmap rows allocated {allocations} blocks");
    assert!(
        allocations <= MAX_CONFIGMAP_ALLOCATIONS,
        "configmap rows allocated {allocations} blocks"
    );
    black_box((&rows, &index, &configmaps));
    drop(configmaps);
    drop(index);
    drop(rows);
    drop(profiler);
}
