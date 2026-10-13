use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Context as _;
use clap::Parser;
use fleet::{
    AppearanceArg, Object, Preset, Quit, Screen, WINDOW_HEIGHT, WINDOW_WIDTH, bind_keys,
    parse_object, root_view,
};
use fleet_core::{Backend, ClusterId, Command};
use fleet_demo::DemoBackend;
use fleet_kube::LiveBackend;
use fleet_kube::discovery::DiscoverySources;
use fleet_kube::settings::KubeSettings;
use fleet_kube::store::ClusterStore;
use fleet_ui::theme::{Appearance, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y};
use fleet_ui::{Root, assets};
use gpui::{
    App, AppContext as _, Bounds, Size, TitlebarOptions, WindowBounds, WindowOptions, point, px,
    size,
};
use tracing_subscriber::EnvFilter;

const MIN_WIDTH: f32 = 720.;
const MIN_HEIGHT: f32 = 480.;
const LOG_SNAPSHOTS_VAR: &str = "FLEET_LOG_SNAPSHOTS";

#[derive(Parser, Debug)]
#[command(
    name = "fleet",
    about = "Watch many Kubernetes clusters at once",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("FLEET_GIT_SHA"), ")")
)]
struct Args {
    #[arg(long)]
    demo: bool,
    #[arg(long, value_enum, default_value_t = Screen::Fleet)]
    screen: Screen,
    #[arg(long, value_enum)]
    appearance: Option<AppearanceArg>,
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long, value_name = "NAME")]
    cluster: Option<String>,
    #[arg(long, value_name = "NAMESPACE/NAME", value_parser = parse_object)]
    object: Option<Object>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let args = Args::parse();
    let (update_tx, update_rx) = async_channel::unbounded();
    let (command_tx, command_rx) = async_channel::unbounded();
    let backend: Box<dyn Backend> = if args.demo {
        Box::new(DemoBackend)
    } else {
        let sources = DiscoverySources::from_env();
        let store = cluster_store(args.data_dir.clone(), &sources)?;
        Box::new(LiveBackend::new(KubeSettings::default(), sources, store))
    };
    backend.start(update_tx, command_rx);
    let update_rx = if std::env::var_os(LOG_SNAPSHOTS_VAR).is_some() {
        log_snapshots(update_rx)
    } else {
        update_rx
    };

    let forced = args.appearance.map(Appearance::from);
    let screen = args.screen;
    let preset = Preset {
        cluster: args.cluster.map(ClusterId::new),
        object: args.object,
    };
    let failure = Rc::new(RefCell::new(None));
    let failed = failure.clone();
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            if let Err(error) = launch(cx, forced, update_rx, command_tx, screen, &preset) {
                failed.replace(Some(error));
                cx.quit();
            }
        });
    failure.take().map_or(Ok(()), Err)
}

fn log_snapshots(
    updates: async_channel::Receiver<fleet_core::Update>,
) -> async_channel::Receiver<fleet_core::Update> {
    let (forward_tx, forward_rx) = async_channel::unbounded();
    std::thread::spawn(move || {
        while let Ok(update) = updates.recv_blocking() {
            if let fleet_core::Update::Snapshot(snapshot) = &update {
                let connected = snapshot
                    .clusters
                    .iter()
                    .filter(|c| c.is_connected())
                    .count();
                let problems = &snapshot.home.problems;
                tracing::info!(
                    clusters = snapshot.clusters.len(),
                    connected,
                    problems = problems.len(),
                    "snapshot"
                );
                for problem in problems {
                    let target = &problem.target;
                    tracing::info!(
                        cluster = %target.cluster.as_str(),
                        kind = %target.kind,
                        namespace = target.namespace.as_deref().unwrap_or_default(),
                        name = %target.name,
                        "problem"
                    );
                }
            }
            if forward_tx.send_blocking(update).is_err() {
                break;
            }
        }
    });
    forward_rx
}

fn cluster_store(
    data_dir: Option<PathBuf>,
    sources: &DiscoverySources,
) -> anyhow::Result<ClusterStore> {
    let dir = match (data_dir, &sources.home) {
        (Some(dir), _) => dir,
        (None, Some(home)) => ClusterStore::default_dir(home),
        (None, None) => anyhow::bail!("no home directory; pass --data-dir"),
    };
    ClusterStore::open(&dir).with_context(|| format!("reading saved clusters in {}", dir.display()))
}

fn launch(
    cx: &mut App,
    forced: Option<Appearance>,
    updates: async_channel::Receiver<fleet_core::Update>,
    commands: async_channel::Sender<Command>,
    screen: Screen,
    preset: &Preset,
) -> anyhow::Result<()> {
    fleet_ui::init(forced, cx)?;
    bind_keys(cx);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_window_closed(|cx, _| cx.quit()).detach();

    let window_size = size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT));
    let bounds = Bounds::centered(None, window_size, cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y)),
        }),
        window_min_size: Some(Size {
            width: px(MIN_WIDTH),
            height: px(MIN_HEIGHT),
        }),
        ..WindowOptions::default()
    };
    cx.open_window(options, |window, cx| {
        let view = root_view(screen, preset, updates, commands, window, cx);
        cx.new(|cx| Root::new(view, window, cx))
    })
    .context("opening the Fleet window")?;
    cx.activate(true);
    Ok(())
}
