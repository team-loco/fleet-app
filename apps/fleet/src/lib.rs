use clap::ValueEnum;
use fleet_core::{AddClusterStep, ClusterId, Command, FluxKey, FluxKind, Page, ResourceRef};
use fleet_ui::actions::{
    CloseWindow, CompareAcrossClusters, FollowLogs, OpenSelectedNewWindow, OpenSettings,
    ShowFluxOwner, ToggleClusterSwitcher, ToggleCommandPalette, ToggleSidebar,
};
use fleet_ui::theme::Appearance;
use fleet_ui::{Gallery, GalleryPage, Shell, Store};
use gpui::{AnyView, App, AppContext as _, Entity, KeyBinding, Menu, MenuItem, Window};

pub const WINDOW_WIDTH: f32 = 1440.;
pub const WINDOW_HEIGHT: f32 = 900.;
const SAMPLE_CLUSTER: &str = "prod-us-east";
const SAMPLE_QUERY: &str = "checkout";
const SAMPLE_RELEASE_CLUSTER: &str = "staging";
const SAMPLE_NAMESPACE: &str = "payments";
const SAMPLE_DEPLOYMENT: &str = "checkout-api";
const DEPLOYMENT: &str = "Deployment";

gpui::actions!(fleet, [Quit, NewWindow]);

pub const APP_ID: &str = "dev.fleet.app";
pub const APP_NAME: &str = "Fleet";
const SETTINGS_ITEM: &str = "Settings…";
const NEW_WINDOW_ITEM: &str = "New Window";
const QUIT_ITEM: &str = "Quit Fleet";
const BUNDLE_EXTENSION: &str = "app";

pub fn menus() -> Vec<Menu> {
    vec![Menu::new(APP_NAME).items([
        MenuItem::action(SETTINGS_ITEM, OpenSettings),
        MenuItem::separator(),
        MenuItem::action(NEW_WINDOW_ITEM, NewWindow),
        MenuItem::separator(),
        MenuItem::action(QUIT_ITEM, Quit),
    ])]
}

pub fn bundled(executable: &std::path::Path) -> bool {
    executable
        .ancestors()
        .any(|dir| dir.extension().is_some_and(|ext| ext == BUNDLE_EXTENSION))
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-shift-c", ToggleClusterSwitcher, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-k", ToggleCommandPalette, None),
        KeyBinding::new("cmd-l", FollowLogs, None),
        KeyBinding::new("cmd-shift-d", CompareAcrossClusters, None),
        KeyBinding::new("cmd-shift-g", ShowFluxOwner, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-n", NewWindow, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("alt-enter", OpenSelectedNewWindow, Some("CommandPalette")),
    ]);
    cx.bind_keys(fleet_ui::screens::workload::key_bindings());
    cx.bind_keys(fleet_ui::key_bindings());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub namespace: String,
    pub name: String,
}

pub fn parse_object(text: &str) -> Result<Object, String> {
    match text.split_once('/') {
        Some((namespace, name)) if !namespace.is_empty() && !name.is_empty() => Ok(Object {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        }),
        _ => Err(format!("expected NAMESPACE/NAME, got {text}")),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Preset {
    pub cluster: Option<ClusterId>,
    pub object: Option<Object>,
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
pub enum Screen {
    Fleet,
    Cluster,
    Collapsed,
    Switcher,
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
    Settings,
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
    pub fn commands(self, preset: &Preset) -> Vec<Command> {
        let cluster = preset.cluster_or(SAMPLE_CLUSTER);
        let sample = Command::SelectCluster(Some(cluster.clone()));
        match self {
            Self::Fleet => vec![Command::SelectPage(Page::Overview)],
            Self::Cluster => vec![sample],
            Self::Collapsed => vec![sample, Command::ToggleSidebar],
            Self::Switcher => vec![sample],
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
            Self::Settings => vec![Command::SelectPage(Page::Settings)],
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
    vec![Command::OpenFluxObject(key)]
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum AppearanceArg {
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

pub fn root_view(
    screen: Screen,
    preset: &Preset,
    store: Entity<Store>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    match screen {
        Screen::Kit => cx
            .new(|cx| Gallery::new(GalleryPage::Primitives, window, cx))
            .into(),
        Screen::KitOverlays => cx
            .new(|cx| Gallery::new(GalleryPage::Overlays, window, cx))
            .into(),
        Screen::KitPanes => cx
            .new(|cx| Gallery::new(GalleryPage::Panes, window, cx))
            .into(),
        Screen::Fleet => shell_view(store, screen, preset, window, cx),
        Screen::Cluster => shell_view(store, screen, preset, window, cx),
        Screen::Collapsed => shell_view(store, screen, preset, window, cx),
        Screen::Switcher => shell_view(store, screen, preset, window, cx),
        Screen::Command => shell_view(store, screen, preset, window, cx),
        Screen::Changes => shell_view(store, screen, preset, window, cx),
        Screen::Overview => shell_view(store, screen, preset, window, cx),
        Screen::Nodes => shell_view(store, screen, preset, window, cx),
        Screen::Namespaces => shell_view(store, screen, preset, window, cx),
        Screen::Events => shell_view(store, screen, preset, window, cx),
        Screen::Resources => shell_view(store, screen, preset, window, cx),
        Screen::AddCluster => shell_view(store, screen, preset, window, cx),
        Screen::AddClusterCheck => shell_view(store, screen, preset, window, cx),
        Screen::Gitops => shell_view(store, screen, preset, window, cx),
        Screen::Workload => shell_view(store, screen, preset, window, cx),
        Screen::Pods => shell_view(store, screen, preset, window, cx),
        Screen::PodsShell => shell_view(store, screen, preset, window, cx),
        Screen::Kustomization => shell_view(store, screen, preset, window, cx),
        Screen::Helmrelease => shell_view(store, screen, preset, window, cx),
        Screen::Source => shell_view(store, screen, preset, window, cx),
        #[cfg(feature = "loco")]
        Screen::LocoPlatform => shell_view(store, screen, preset, window, cx),
        #[cfg(feature = "loco")]
        Screen::LocoApps => shell_view(store, screen, preset, window, cx),
        #[cfg(feature = "loco")]
        Screen::LocoBuilds => shell_view(store, screen, preset, window, cx),
        Screen::Compare => shell_view(store, screen, preset, window, cx),
        Screen::ClusterSettings => shell_view(store, screen, preset, window, cx),
        Screen::Settings => shell_view(store, screen, preset, window, cx),
        #[cfg(feature = "loco")]
        Screen::LocoRoutes => shell_view(store, screen, preset, window, cx),
    }
}

fn shell_view(
    store: Entity<Store>,
    screen: Screen,
    preset: &Preset,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let initial = screen.commands(preset);
    let shell = cx.new(|cx| Shell::new(store, initial, window, cx));
    if let Screen::Command = screen {
        shell.update(cx, |shell, cx| shell.open_palette(SAMPLE_QUERY, window, cx));
    }
    if let Screen::Switcher = screen {
        shell.update(cx, |shell, cx| shell.open_switcher(window, cx));
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
    fn only_executables_inside_an_app_bundle_count_as_bundled() {
        use std::path::Path;
        assert!(bundled(Path::new(
            "/Applications/Fleet.app/Contents/MacOS/fleet"
        )));
        assert!(!bundled(Path::new("/Users/me/fleet/target/debug/fleet")));
    }

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
            [Command::OpenFluxObject(key)]
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
