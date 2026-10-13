use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Context as _;
use clap::Parser;
use fleet::{
    AppearanceArg, NewWindow, Object, Preset, Quit, Screen, WINDOW_HEIGHT, WINDOW_WIDTH, bind_keys,
    parse_object, root_view,
};
use fleet_core::{Backend, ClusterId, Command};
use fleet_demo::DemoBackend;
use fleet_kube::LiveBackend;
use fleet_kube::discovery::DiscoverySources;
use fleet_kube::settings::KubeSettings;
use fleet_kube::store::ClusterStore;
use fleet_ui::theme::{Appearance, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y};
use fleet_ui::{NewWindowRequest, Root, Shell, Store, assets};
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
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    let store = cx.new(|cx| Store::new(updates, commands, cx));
    let new_window_store = store.clone();
    cx.on_action(move |_: &NewWindow, cx| {
        if let Err(error) = open_fleet_window(new_window_store.clone(), Vec::new(), cx) {
            tracing::error!(%error, "opening a Fleet window");
        }
    });
    open_startup_window(store, screen, preset, cx)
}

fn window_options(cx: &App) -> WindowOptions {
    let window_size = size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT));
    let bounds = Bounds::centered(None, window_size, cx);
    WindowOptions {
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
    }
}

fn open_startup_window(
    store: gpui::Entity<Store>,
    screen: Screen,
    preset: &Preset,
    cx: &mut App,
) -> anyhow::Result<()> {
    let options = window_options(cx);
    cx.open_window(options, |window, cx| {
        let view = root_view(screen, preset, store.clone(), window, cx);
        if let Ok(shell) = view.clone().downcast::<Shell>() {
            subscribe_windows(&shell, store, cx);
        }
        cx.new(|cx| Root::new(view, window, cx))
    })
    .context("opening the Fleet window")?;
    cx.activate(true);
    Ok(())
}

fn subscribe_windows(shell: &gpui::Entity<Shell>, store: gpui::Entity<Store>, cx: &mut App) {
    cx.subscribe(shell, move |_, request: &NewWindowRequest, cx| {
        if let Err(error) = open_fleet_window(store.clone(), request.0.clone(), cx) {
            tracing::error!(%error, "opening a Fleet window");
        }
    })
    .detach();
}

fn open_fleet_window(
    store: gpui::Entity<Store>,
    initial: Vec<Command>,
    cx: &mut App,
) -> anyhow::Result<()> {
    let options = window_options(cx);
    cx.open_window(options, |window, cx| {
        let shell = cx.new(|cx| Shell::new(store.clone(), initial, window, cx));
        subscribe_windows(&shell, store, cx);
        cx.new(|cx| Root::new(shell, window, cx))
    })
    .context("opening the Fleet window")?;
    cx.activate(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn new_window_shortcut_and_close_keep_other_windows_open(cx: &mut gpui::TestAppContext) {
        let (backend, updates) = async_channel::unbounded();
        let (commands, _received) = async_channel::unbounded();
        cx.update(|cx| {
            launch(
                cx,
                Some(Appearance::Dark),
                updates,
                commands,
                Screen::Fleet,
                &Preset::default(),
            )
            .unwrap_or_else(|error| panic!("test launch: {error}"));
        });
        assert!(
            backend
                .try_send(fleet_core::Update::Snapshot(Box::default()))
                .is_ok()
        );
        cx.run_until_parked();
        let first = cx
            .read(|cx| cx.windows().into_iter().next())
            .unwrap_or_else(|| panic!("first window missing"));
        cx.simulate_keystrokes(first, "cmd-n");
        let windows = cx.read(App::windows);
        assert_eq!(windows.len(), 2);
        let second = windows
            .into_iter()
            .find(|window| *window != first)
            .unwrap_or_else(|| panic!("second window missing"));
        cx.simulate_keystrokes(first, "cmd-w");
        assert_eq!(cx.read(App::windows), vec![second]);
        cx.simulate_keystrokes(second, "cmd-n");
        assert_eq!(cx.read(|cx| cx.windows().len()), 2);
    }
}
