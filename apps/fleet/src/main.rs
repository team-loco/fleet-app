use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Context as _;
use clap::{Parser, ValueEnum};
use fleet_core::{
    AddClusterStep, Backend, ClusterId, Command, FluxKey, FluxKind, Page, ResourceRef,
};
use fleet_demo::DemoBackend;
use fleet_kube::LiveBackend;
use fleet_kube::discovery::DiscoverySources;
use fleet_kube::settings::KubeSettings;
use fleet_kube::store::ClusterStore;
use fleet_ui::actions::{
    CompareAcrossClusters, FollowLogs, ShowFluxOwner, ToggleClustersPanel, ToggleCommandPalette,
    ToggleResourcesPanel,
};
use fleet_ui::theme::{Appearance, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y};
use fleet_ui::{Gallery, GalleryPage, Root, Shell, assets};
use gpui::{
    App, AppContext as _, Bounds, KeyBinding, Size, TitlebarOptions, WindowBounds, WindowOptions,
    point, px, size,
};
use tracing_subscriber::EnvFilter;

const WINDOW_WIDTH: f32 = 1440.;
const WINDOW_HEIGHT: f32 = 900.;
const MIN_WIDTH: f32 = 720.;
const MIN_HEIGHT: f32 = 480.;
const SAMPLE_CLUSTER: &str = "prod-us-east";
const SAMPLE_QUERY: &str = "checkout";
const SAMPLE_RELEASE_CLUSTER: &str = "staging";
const SAMPLE_NAMESPACE: &str = "payments";
const SAMPLE_DEPLOYMENT: &str = "checkout-api";
const DEPLOYMENT: &str = "Deployment";

gpui::actions!(fleet, [Quit]);

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Object {
    namespace: String,
    name: String,
}

fn parse_object(text: &str) -> Result<Object, String> {
    match text.split_once('/') {
        Some((namespace, name)) if !namespace.is_empty() && !name.is_empty() => Ok(Object {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        }),
        _ => Err(format!("expected NAMESPACE/NAME, got {text}")),
    }
}

#[derive(Clone, Debug, Default)]
struct Preset {
    cluster: Option<ClusterId>,
    object: Option<Object>,
}

impl Preset {
    fn cluster_or(&self, sample: &str) -> ClusterId {
        self.cluster
            .clone()
            .unwrap_or_else(|| ClusterId::new(sample))
    }

    fn object_or(&self, namespace: &str, name: &str) -> Object {
        self.object.clone().unwrap_or_else(|| Object {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        })
    }

    fn flux(&self, sample_cluster: &str, kind: FluxKind, namespace: &str, name: &str) -> FluxKey {
        let object = self.object_or(namespace, name);
        FluxKey::new(
            self.cluster_or(sample_cluster).as_str(),
            kind,
            object.namespace,
            object.name,
        )
    }

    fn deployment(&self) -> ResourceRef {
        let object = self.object_or(SAMPLE_NAMESPACE, SAMPLE_DEPLOYMENT);
        ResourceRef {
            cluster: self.cluster_or(SAMPLE_CLUSTER),
            kind: DEPLOYMENT.into(),
            namespace: Some(object.namespace),
            name: object.name,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Screen {
    Fleet,
    Cluster,
    Collapsed,
    Command,
    Changes,
    Overview,
    Nodes,
    Namespaces,
    Events,
    Resources,
    Gitops,
    Compare,
    ClusterSettings,
    Kit,
    KitOverlays,
    KitPanes,
    AddCluster,
    AddClusterCheck,
    Workload,
    Pods,
    PodsShell,
    Kustomization,
    Helmrelease,
    Source,
    #[cfg(feature = "loco")]
    LocoPlatform,
    #[cfg(feature = "loco")]
    LocoApps,
    #[cfg(feature = "loco")]
    LocoBuilds,
    #[cfg(feature = "loco")]
    LocoRoutes,
}

impl Screen {
    fn commands(self, preset: &Preset) -> Vec<Command> {
        let cluster = preset.cluster_or(SAMPLE_CLUSTER);
        let sample = Command::SelectCluster(Some(cluster.clone()));
        match self {
            Self::Fleet => vec![Command::SelectPage(Page::Overview)],
            Self::Cluster => vec![sample],
            Self::Collapsed => vec![
                sample,
                Command::ToggleClustersPanel,
                Command::ToggleResourcesPanel,
            ],
            Self::Command => vec![Command::SelectPage(Page::Overview)],
            Self::Changes => vec![Command::SelectPage(Page::Changes)],
            Self::Overview => vec![sample, Command::SelectPage(Page::ClusterOverview)],
            Self::Nodes => vec![sample, Command::SelectPage(Page::Nodes)],
            Self::Namespaces => vec![sample, Command::SelectPage(Page::Namespaces)],
            Self::Events => vec![sample, Command::SelectPage(Page::Events)],
            Self::Resources => vec![sample, Command::SelectPage(Page::Deployments)],
            Self::Gitops => vec![Command::SelectPage(Page::GitOps)],
            Self::Compare => vec![Command::Compare(preset.deployment())],
            Self::ClusterSettings => vec![Command::ShowClusterSettings(cluster)],
            Self::Kit => Vec::new(),
            Self::KitOverlays => Vec::new(),
            Self::KitPanes => Vec::new(),
            Self::AddCluster => vec![Command::OpenAddCluster(AddClusterStep::Pick)],
            Self::AddClusterCheck => vec![Command::OpenAddCluster(AddClusterStep::Check)],
            Self::Workload => vec![sample, Command::OpenResource(preset.deployment())],
            Self::Pods => vec![sample, Command::SelectPage(Page::Pods)],
            Self::PodsShell => vec![sample, Command::SelectPage(Page::Pods)],
            Self::Kustomization => flux_screen(preset.flux(
                SAMPLE_CLUSTER,
                FluxKind::Kustomization,
                "flux-system",
                "apps-payments",
            )),
            Self::Helmrelease => flux_screen(preset.flux(
                SAMPLE_RELEASE_CLUSTER,
                FluxKind::HelmRelease,
                "monitoring",
                "kube-prometheus-stack",
            )),
            Self::Source => flux_screen(preset.flux(
                SAMPLE_CLUSTER,
                FluxKind::GitRepository,
                "flux-system",
                "fleet-infra",
            )),
            #[cfg(feature = "loco")]
            Self::LocoPlatform => vec![sample, Command::SelectPage(Page::LocoPlatform)],
            #[cfg(feature = "loco")]
            Self::LocoApps => vec![sample, Command::SelectPage(Page::LocoApplications)],
            #[cfg(feature = "loco")]
            Self::LocoBuilds => vec![sample, Command::SelectPage(Page::LocoBuilds)],
            #[cfg(feature = "loco")]
            Self::LocoRoutes => vec![sample, Command::SelectPage(Page::LocoRoutes)],
        }
    }
}

fn flux_screen(key: FluxKey) -> Vec<Command> {
    vec![Command::OpenFluxObject(key), Command::ToggleClustersPanel]
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AppearanceArg {
    Dark,
    Light,
}

impl From<AppearanceArg> for Appearance {
    fn from(arg: AppearanceArg) -> Self {
        match arg {
            AppearanceArg::Dark => Self::Dark,
            AppearanceArg::Light => Self::Light,
        }
    }
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

fn cluster_store(
    data_dir: Option<PathBuf>,
    sources: &DiscoverySources,
) -> anyhow::Result<ClusterStore> {
    let dir = match (data_dir, &sources.home) {
        (Some(dir), _) => dir,
        (None, Some(home)) => ClusterStore::default_dir(home),
        (None, None) => anyhow::bail!("no home directory; pass --data-dir"),
    };
    match ClusterStore::open(&dir) {
        Ok(store) => Ok(store),
        Err(error) => {
            tracing::error!(%error, dir = %dir.display(), "saved clusters unreadable; starting empty");
            Ok(ClusterStore::empty(&dir))
        }
    }
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
    cx.bind_keys([
        KeyBinding::new("cmd-shift-b", ToggleClustersPanel, None),
        KeyBinding::new("cmd-b", ToggleResourcesPanel, None),
        KeyBinding::new("cmd-k", ToggleCommandPalette, None),
        KeyBinding::new("cmd-l", FollowLogs, None),
        KeyBinding::new("cmd-shift-d", CompareAcrossClusters, None),
        KeyBinding::new("cmd-shift-g", ShowFluxOwner, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    cx.bind_keys(fleet_ui::screens::workload::key_bindings());
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
        let view: gpui::AnyView = match screen {
            Screen::Kit => cx
                .new(|cx| Gallery::new(GalleryPage::Primitives, window, cx))
                .into(),
            Screen::KitOverlays => cx
                .new(|cx| Gallery::new(GalleryPage::Overlays, window, cx))
                .into(),
            Screen::KitPanes => cx
                .new(|cx| Gallery::new(GalleryPage::Panes, window, cx))
                .into(),
            Screen::Fleet => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Cluster => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Collapsed => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Command => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Changes => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Overview => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Nodes => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Namespaces => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Events => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Resources => shell_view(updates, commands, screen, preset, window, cx),
            Screen::AddCluster => shell_view(updates, commands, screen, preset, window, cx),
            Screen::AddClusterCheck => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Gitops => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Workload => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Pods => shell_view(updates, commands, screen, preset, window, cx),
            Screen::PodsShell => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Kustomization => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Helmrelease => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Source => shell_view(updates, commands, screen, preset, window, cx),
            #[cfg(feature = "loco")]
            Screen::LocoPlatform => shell_view(updates, commands, screen, preset, window, cx),
            #[cfg(feature = "loco")]
            Screen::LocoApps => shell_view(updates, commands, screen, preset, window, cx),
            #[cfg(feature = "loco")]
            Screen::LocoBuilds => shell_view(updates, commands, screen, preset, window, cx),
            Screen::Compare => shell_view(updates, commands, screen, preset, window, cx),
            Screen::ClusterSettings => shell_view(updates, commands, screen, preset, window, cx),
            #[cfg(feature = "loco")]
            Screen::LocoRoutes => shell_view(updates, commands, screen, preset, window, cx),
        };
        cx.new(|cx| Root::new(view, window, cx))
    })
    .context("opening the Fleet window")?;
    cx.activate(true);
    Ok(())
}

fn shell_view(
    updates: async_channel::Receiver<fleet_core::Update>,
    commands: async_channel::Sender<Command>,
    screen: Screen,
    preset: &Preset,
    window: &mut gpui::Window,
    cx: &mut App,
) -> gpui::AnyView {
    let initial = screen.commands(preset);
    let shell = cx.new(|cx| Shell::new(updates, commands, initial, window, cx));
    if let Screen::Command = screen {
        shell.update(cx, |shell, cx| shell.open_palette(SAMPLE_QUERY, window, cx));
    }
    if let Screen::PodsShell = screen {
        shell.update(cx, |shell, cx| shell.show_pod_terminal(window, cx));
    }
    shell.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_parse_as_namespace_and_name() {
        assert_eq!(
            parse_object("payments/checkout-api"),
            Ok(Object {
                namespace: "payments".into(),
                name: "checkout-api".into(),
            })
        );
        assert!(parse_object("checkout-api").is_err());
        assert!(parse_object("/x").is_err());
    }

    #[test]
    fn presets_name_the_live_cluster_and_object() {
        let preset = Preset {
            cluster: Some(ClusterId::new("kind-fleet-a")),
            object: parse_object("apps/podinfo").ok(),
        };
        let key = FluxKey::new("kind-fleet-a", FluxKind::HelmRelease, "apps", "podinfo");
        assert_eq!(
            Screen::Helmrelease.commands(&preset),
            [Command::OpenFluxObject(key), Command::ToggleClustersPanel]
        );
        let workload = Screen::Workload.commands(&preset);
        assert_eq!(
            workload.last(),
            Some(&Command::OpenResource(ResourceRef {
                cluster: ClusterId::new("kind-fleet-a"),
                kind: DEPLOYMENT.into(),
                namespace: Some("apps".into()),
                name: "podinfo".into(),
            }))
        );
    }

    #[test]
    fn presets_default_to_the_sample_data() {
        let preset = Preset::default();
        let release = Screen::Helmrelease.commands(&preset);
        let Some(Command::OpenFluxObject(key)) = release.first() else {
            panic!("expected a Flux object");
        };
        assert_eq!(key.cluster, ClusterId::new(SAMPLE_RELEASE_CLUSTER));
        assert_eq!(
            Screen::Nodes.commands(&preset).first(),
            Some(&Command::SelectCluster(Some(ClusterId::new(
                SAMPLE_CLUSTER
            ))))
        );
    }
}
