use fleet::Screen;
use fleet_core::resources::EVENT_LIST;
use fleet_core::{ClusterId, ClusterSummary, Command, Page};
use fleet_shots::Ui;
use fleet_ui::Shell;
use fleet_ui::theme::{SIDEBAR, SIDEBAR_COLLAPSED};

const EXPECTED_WIDTH: u32 = 2880;
const EXPECTED_HEIGHT: u32 = 1800;

#[test]
fn command_k_opens_the_palette_and_enter_runs_the_top_hit() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Fleet)?;
    assert_eq!(ui.state().page, Page::Overview);
    ui.press("cmd-k")?;
    ui.type_text("gitops")?;
    ui.press("enter")?;
    assert_eq!(ui.state().page, Page::GitOps);
    Ok(())
}

#[test]
fn command_b_collapses_the_sidebar_to_a_narrow_strip() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    let before = ui.state();
    assert!(before.sidebar_open);
    let open = ui.bounds("sidebar").map(|bounds| bounds.size.width);
    assert_eq!(open, Some(SIDEBAR));
    ui.press("cmd-b")?;
    assert!(!ui.state().sidebar_open);
    assert_eq!(ui.state().cluster, before.cluster);
    let collapsed = ui.bounds("sidebar").map(|bounds| bounds.size.width);
    assert_eq!(collapsed, Some(SIDEBAR_COLLAPSED));
    assert!(ui.bounds("expand-sidebar").is_some());
    ui.press("cmd-b")?;
    assert!(ui.state().sidebar_open);
    Ok(())
}

#[test]
fn the_sidebar_buttons_collapse_and_expand_it() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    ui.click("collapse-sidebar")?;
    assert!(!ui.state().sidebar_open);
    ui.click("expand-sidebar")?;
    assert!(ui.state().sidebar_open);
    Ok(())
}

fn cluster_at(ui: &Ui, ix: usize) -> anyhow::Result<ClusterSummary> {
    ui.state()
        .snapshot
        .clusters
        .get(ix)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("the demo has no cluster {ix}"))
}

#[test]
fn clicking_the_switcher_opens_it_and_a_row_selects_that_cluster() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Fleet)?;
    let second = cluster_at(&ui, 1)?;
    let connected = ui.state().snapshot.connected();
    assert!(!ui.read(Shell::switcher_open));
    ui.click("cluster-switcher")?;
    assert!(ui.read(Shell::switcher_open));
    ui.click("switcher-row-2")?;
    let state = ui.state();
    assert_eq!(state.cluster, Some(second.id));
    assert_eq!(state.page, Page::ClusterOverview);
    assert!(!ui.read(Shell::switcher_open));
    assert_eq!(state.snapshot.connected(), connected);
    Ok(())
}

#[test]
fn typing_in_the_switcher_filters_and_enter_selects() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Fleet)?;
    let staging = ClusterId::new("staging");
    ui.press("cmd-shift-c")?;
    assert!(ui.read(Shell::switcher_open));
    ui.type_text("stag")?;
    ui.press("enter")?;
    assert_eq!(ui.state().cluster, Some(staging));
    ui.press("cmd-shift-c")?;
    ui.type_text("all")?;
    ui.press("enter")?;
    let state = ui.state();
    assert_eq!(state.cluster, None);
    assert_eq!(state.page, Page::Overview);
    Ok(())
}

#[test]
fn arrow_keys_move_from_the_current_cluster() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    let first = cluster_at(&ui, 0)?;
    let second = cluster_at(&ui, 1)?;
    assert_eq!(ui.state().cluster, Some(first.id));
    ui.press("cmd-shift-c down enter")?;
    assert_eq!(ui.state().cluster, Some(second.id));
    ui.press("cmd-shift-c up up enter")?;
    assert_eq!(ui.state().cluster, None);
    ui.press("cmd-shift-c escape")?;
    assert!(!ui.read(Shell::switcher_open));
    assert_eq!(ui.state().cluster, None);
    Ok(())
}

#[test]
fn cluster_actions_open_from_the_switcher() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    ui.press("cmd-shift-c cmd-.")?;
    assert!(ui.bounds("switcher-back").is_some());
    ui.press("escape")?;
    assert!(ui.bounds("switcher-back").is_none());
    assert!(ui.read(Shell::switcher_open));
    ui.press("cmd-.")?;
    ui.click("switcher-action-0")?;
    assert_eq!(ui.state().page, Page::ClusterSettings);
    assert!(!ui.read(Shell::switcher_open));
    Ok(())
}

#[cfg(feature = "loco")]
#[test]
fn the_loco_section_shows_only_for_clusters_running_loco() -> anyhow::Result<()> {
    let platform: &'static str = format!("page-{}", Page::LocoPlatform as u64).leak();
    let mut ui = Ui::open(Screen::Cluster)?;
    assert!(ui.bounds(platform).is_some());
    ui.press("cmd-shift-c")?;
    ui.type_text("staging")?;
    ui.press("enter")?;
    assert_eq!(ui.state().cluster, Some(ClusterId::new("staging")));
    assert!(ui.bounds(platform).is_none());
    Ok(())
}

#[test]
fn cancelling_a_confirm_dialog_leaves_the_state_unchanged() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Workload)?;
    let before = ui.state();
    ui.click("workload-actions")?;
    ui.click("workload-actions-item-4")?;
    assert!(ui.bounds("dialog-cancel").is_some());
    ui.click("dialog-cancel")?;
    assert!(ui.bounds("dialog-cancel").is_none());
    assert_eq!(ui.state(), before);
    Ok(())
}

#[test]
fn clicking_a_cluster_row_selects_that_cluster() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Fleet)?;
    let second: ClusterId = ui
        .state()
        .snapshot
        .clusters
        .get(1)
        .map(|cluster| cluster.id.clone())
        .ok_or_else(|| anyhow::anyhow!("the demo has fewer than two clusters"))?;
    ui.click("fleet-clusters-row-1")?;
    let state = ui.state();
    assert_eq!(state.cluster, Some(second));
    assert_eq!(state.page, Page::ClusterOverview);
    Ok(())
}

#[test]
fn save_png_renders_the_state_the_steps_reached() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    ui.press("cmd-k")?;
    ui.type_text("nodes")?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("palette.png");
    ui.save_png(&path)?;
    let (width, height) = image::image_dimensions(&path)?;
    assert_eq!((width, height), (EXPECTED_WIDTH, EXPECTED_HEIGHT));
    Ok(())
}

#[test]
fn option_enter_requests_a_window_with_the_cluster_page_without_navigating_the_source()
-> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    let before = ui.state();
    ui.press("cmd-k")?;
    ui.type_text("nodes")?;
    ui.press("alt-enter")?;
    assert_eq!(
        ui.take_new_window(),
        Some(vec![
            Command::SelectCluster(before.cluster.clone()),
            Command::SelectPage(Page::Nodes)
        ])
    );
    assert_eq!(ui.take_new_window(), None);
    assert_eq!(ui.state(), before);
    Ok(())
}

#[test]
fn command_comma_opens_settings_and_its_controls_change_the_app_settings() -> anyhow::Result<()> {
    use fleet_core::app_settings::{PollInterval, ThemeChoice};
    let mut ui = Ui::open(Screen::Cluster)?;
    ui.press("cmd-,")?;
    assert_eq!(ui.state().page, Page::Settings);
    let current = |ui: &Ui| ui.read(|_, cx| fleet_ui::settings::current(cx).clone());
    assert_eq!(current(&ui).theme, ThemeChoice::System);
    ui.click("settings-theme-2")?;
    assert_eq!(current(&ui).theme, ThemeChoice::Dark);
    ui.click("settings-metrics-0")?;
    assert_eq!(current(&ui).refresh.metrics, PollInterval::Seconds5);
    assert!(current(&ui).notifications.enabled);
    ui.click("settings-notify")?;
    assert!(!current(&ui).notifications.enabled);
    ui.click("settings-clusters")?;
    assert_eq!(ui.state().page, Page::ClusterSettings);
    Ok(())
}

const DEPLOYMENTS: &str = "resources-Deployment";
const SAMPLE: &str = "prod-us-east";
const DRAG: f32 = 80.;
const ROWS_UNDER_POPOVER: usize = 8;
const AVAILABLE: usize = 4;
const AVAILABLE_DEFAULT: f32 = 72.;

fn selector(text: String) -> &'static str {
    text.leak()
}

fn handle_x(ui: &mut Ui, column: usize) -> anyhow::Result<f32> {
    ui.bounds(selector(format!("{DEPLOYMENTS}-resize-{column}")))
        .map(|bounds| f32::from(bounds.origin.x))
        .ok_or_else(|| anyhow::anyhow!("column {column} has no resize handle"))
}

fn saved_width(ui: &Ui, scope: &str, column: &str) -> Option<f32> {
    ui.read_app(|cx| fleet_ui::prefs::column_widths(scope, DEPLOYMENTS, cx))
        .get(column)
        .copied()
}

#[test]
fn dragging_a_column_border_resizes_it_and_remembers_it_per_cluster() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Resources)?;
    let before = handle_x(&mut ui, 0)?;
    ui.drag(
        selector(format!("{DEPLOYMENTS}-resize-0")),
        gpui::point(gpui::px(DRAG), gpui::px(0.)),
    )?;
    let after = handle_x(&mut ui, 0)?;
    assert!((after - before - DRAG).abs() < 1., "{before} → {after}");
    let saved = saved_width(&ui, SAMPLE, "Name");
    assert!(saved.is_some_and(|width| width > DRAG));
    assert_eq!(saved_width(&ui, "staging", "Name"), None);
    ui.press("cmd-shift-c")?;
    ui.type_text("staging")?;
    ui.press("enter")?;
    ui.press("cmd-k")?;
    ui.type_text("deployments")?;
    ui.press("enter")?;
    assert_eq!(ui.state().page, Page::Deployments);
    assert!((handle_x(&mut ui, 0)? - before).abs() < 1.);
    ui.press("cmd-shift-c")?;
    ui.type_text(SAMPLE)?;
    ui.press("enter")?;
    ui.press("cmd-k")?;
    ui.type_text("deployments")?;
    ui.press("enter")?;
    assert!((handle_x(&mut ui, 0)? - after).abs() < 1.);
    Ok(())
}

#[test]
fn double_clicking_a_column_border_fits_the_column_to_its_content() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Resources)?;
    let before = handle_x(&mut ui, AVAILABLE)?;
    ui.double_click(selector(format!("{DEPLOYMENTS}-resize-{AVAILABLE}")))?;
    let fitted = saved_width(&ui, SAMPLE, "Available")
        .ok_or_else(|| anyhow::anyhow!("the fitted width was not saved"))?;
    assert!(fitted >= f32::from(fleet_ui::theme::COLUMN_MIN));
    assert!(fitted < AVAILABLE_DEFAULT, "{fitted}");
    let after = handle_x(&mut ui, AVAILABLE)?;
    assert!(after < before, "{before} → {after}");
    Ok(())
}

fn filter_words(ui: &Ui) -> Option<Vec<String>> {
    ui.read(|shell, cx| shell.filter_bar(cx).map(|bar| bar.read(cx).words()))
}

#[test]
fn slash_focuses_the_filter_and_escape_clears_it() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Resources)?;
    assert!(
        ui.bounds(selector(format!("{DEPLOYMENTS}-row-1")))
            .is_some()
    );
    ui.press("/")?;
    ui.type_text("checkout")?;
    assert_eq!(filter_words(&ui), Some(vec!["checkout".to_owned()]));
    assert!(
        ui.bounds(selector(format!("{DEPLOYMENTS}-row-0")))
            .is_some()
    );
    assert!(
        ui.bounds(selector(format!("{DEPLOYMENTS}-row-1")))
            .is_none()
    );
    assert!(ui.bounds("resource-filter-clear").is_some());
    ui.press("escape")?;
    assert_eq!(filter_words(&ui), Some(Vec::new()));
    assert!(
        ui.bounds(selector(format!("{DEPLOYMENTS}-row-1")))
            .is_some()
    );
    assert!(ui.bounds("resource-filter-clear").is_none());
    Ok(())
}

#[test]
fn hovering_an_open_popover_does_not_hover_the_rows_beneath() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Resources)?;
    ui.hover(selector(format!("{DEPLOYMENTS}-row-2")))?;
    assert!(
        ui.bounds(selector(format!("{DEPLOYMENTS}-row-2-more")))
            .is_some()
    );
    ui.click("resource-namespace")?;
    for option in 1..=3 {
        ui.hover(selector(format!("resource-namespace-option-{option}")))?;
        for row in 0..ROWS_UNDER_POPOVER {
            let more = selector(format!("{DEPLOYMENTS}-row-{row}-more"));
            assert!(
                ui.bounds(more).is_none(),
                "row {row} is hovered under option {option}"
            );
        }
    }
    Ok(())
}

#[test]
fn right_clicking_a_row_opens_its_actions_and_destructive_ones_confirm() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Resources)?;
    let before = ui.state();
    ui.right_click(selector(format!("{DEPLOYMENTS}-row-1")))?;
    ui.click(selector(format!("{DEPLOYMENTS}-row-menu-item-0")))?;
    assert!(ui.bounds("dialog-cancel").is_some());
    ui.click("dialog-cancel")?;
    assert_eq!(ui.state(), before);
    ui.hover(selector(format!("{DEPLOYMENTS}-row-3")))?;
    ui.click(selector(format!("{DEPLOYMENTS}-row-3-more")))?;
    assert!(
        ui.bounds(selector(format!("{DEPLOYMENTS}-row-menu-item-0")))
            .is_some()
    );
    ui.press("escape")?;
    assert!(
        ui.bounds(selector(format!("{DEPLOYMENTS}-row-menu-item-0")))
            .is_none()
    );
    Ok(())
}

#[test]
fn command_p_quick_opens_a_page_and_command_shift_p_runs_a_command() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    ui.press("cmd-p")?;
    ui.type_text("nodes")?;
    ui.press("enter")?;
    assert_eq!(ui.state().page, Page::Nodes);
    assert!(ui.state().sidebar_open);
    ui.press("cmd-shift-p")?;
    ui.type_text("toggle sidebar")?;
    ui.press("enter")?;
    assert!(!ui.state().sidebar_open);
    Ok(())
}

#[test]
fn back_forward_and_control_tab_walk_the_visited_pages() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    assert_eq!(ui.state().page, Page::ClusterOverview);
    ui.press("cmd-p")?;
    ui.type_text("nodes")?;
    ui.press("enter")?;
    ui.press("cmd-p")?;
    ui.type_text("events")?;
    ui.press("enter")?;
    assert_eq!(ui.state().page, Page::Events);
    ui.press("cmd-[")?;
    assert_eq!(ui.state().page, Page::Nodes);
    ui.press("cmd-[")?;
    assert_eq!(ui.state().page, Page::ClusterOverview);
    ui.press("cmd-]")?;
    assert_eq!(ui.state().page, Page::Nodes);
    ui.press("ctrl-tab")?;
    assert_eq!(ui.state().page, Page::ClusterOverview);
    ui.press("ctrl-tab")?;
    assert_eq!(ui.state().page, Page::Events);
    ui.press("ctrl-shift-tab")?;
    assert_eq!(ui.state().page, Page::ClusterOverview);
    Ok(())
}

#[test]
fn arrow_keys_home_end_and_type_ahead_move_through_a_table() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Fleet)?;
    let clusters: Vec<ClusterId> = ui
        .state()
        .snapshot
        .clusters
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let (Some(first), Some(second), Some(last)) =
        (clusters.first(), clusters.get(1), clusters.last())
    else {
        anyhow::bail!("the demo has too few clusters");
    };
    let mut open = |keys: &str| -> anyhow::Result<Option<ClusterId>> {
        ui.press(&format!("cmd-2 {keys} enter"))?;
        let cluster = ui.state().cluster;
        ui.press("cmd-[")?;
        assert_eq!(ui.state().page, Page::Overview);
        Ok(cluster)
    };
    assert_eq!(open("down")?.as_ref(), Some(first));
    assert_eq!(open("home down")?.as_ref(), Some(second));
    assert_eq!(open("end")?.as_ref(), Some(last));
    assert_eq!(open("end up home")?.as_ref(), Some(first));
    assert_eq!(open("home pagedown")?.as_ref(), Some(last));
    assert_eq!(open("end pageup")?.as_ref(), Some(first));
    ui.press("cmd-2 home")?;
    ui.type_text("stag")?;
    ui.press("enter")?;
    assert_eq!(ui.state().cluster, Some(ClusterId::new("staging")));
    Ok(())
}

#[test]
fn escape_and_command_w_close_the_inspector_before_the_window() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Nodes)?;
    ui.press("cmd-2 down")?;
    assert!(ui.bounds("close-node").is_some());
    ui.press("escape")?;
    assert!(ui.bounds("close-node").is_none());
    ui.press("cmd-2 down")?;
    assert!(ui.bounds("close-node").is_some());
    ui.press("cmd-w")?;
    assert!(ui.bounds("close-node").is_none());
    assert_eq!(ui.state().page, Page::Nodes);
    Ok(())
}

#[test]
fn command_j_toggles_the_dock_and_command_w_closes_it_first() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Workload)?;
    let dock = |ui: &Ui| ui.read(Shell::dock_open);
    assert_eq!(dock(&ui), Some(true));
    ui.press("cmd-j")?;
    assert_eq!(dock(&ui), Some(false));
    ui.press("cmd-j")?;
    assert_eq!(dock(&ui), Some(true));
    ui.press("cmd-w")?;
    assert_eq!(dock(&ui), Some(false));
    assert!(ui.state().workload().is_some());
    Ok(())
}

#[test]
fn command_1_focuses_the_sidebar_where_arrows_and_enter_pick_a_page() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    ui.press("cmd-1 down down enter")?;
    assert_eq!(ui.state().page, Page::Namespaces);
    ui.press("cmd-1 end enter")?;
    let last = ui.read(|shell, _| {
        shell
            .state()
            .snapshot
            .nav_sections(shell.state().cluster.as_ref())
            .last()
            .and_then(|s| s.pages.last().copied())
    });
    assert_eq!(Some(ui.state().page), last);
    ui.press("cmd-1 home")?;
    ui.type_text("ev")?;
    ui.press("enter")?;
    assert_eq!(ui.state().page, Page::Events);
    Ok(())
}

#[test]
fn event_row_opens_inspector_enter_reopens_and_escape_closes() -> anyhow::Result<()> {
    for appearance in [
        fleet_ui::theme::Appearance::Dark,
        fleet_ui::theme::Appearance::Light,
    ] {
        let mut ui = Ui::with(Screen::Events, fleet::Preset::default(), appearance)?;
        assert!(ui.bounds("close-event").is_none());
        ui.click("events-row-0")?;
        assert!(ui.bounds("close-event").is_some());
        assert!(ui.bounds("event-object").is_some());
        ui.press("escape")?;
        assert!(ui.bounds("close-event").is_none());
        ui.press("down enter")?;
        assert!(ui.bounds("close-event").is_some());
        ui.click("event-object")?;
        assert_eq!(ui.state().page, Page::Pods);
    }
    Ok(())
}

#[test]
fn inspector_edge_resizes_clamps_and_double_click_resets() -> anyhow::Result<()> {
    use fleet_ui::theme::{INSPECTOR_MAX, INSPECTOR_MIN};
    use gpui::{point, px};
    let mut ui = Ui::open(Screen::ResourceInspector)?;
    let width = |ui: &mut Ui| ui.bounds("resource-inspector-pane").map(|b| b.size.width);
    let original = width(&mut ui).ok_or_else(|| anyhow::anyhow!("no inspector"))?;
    ui.drag("resource-inspector-resize", point(px(-120.), px(0.)))?;
    assert_eq!(width(&mut ui), Some(original + px(120.)));
    assert_eq!(
        ui.read(|_, cx| fleet_ui::settings::current(cx)
            .inspector_widths
            .get("resource-inspector")
            .copied()),
        Some(600)
    );
    ui.click("close-resource")?;
    assert!(ui.bounds("resource-inspector-pane").is_none());
    ui.click("resources-ConfigMap-row-0")?;
    assert_eq!(width(&mut ui), Some(original + px(120.)));
    ui.drag("resource-inspector-resize", point(px(-2000.), px(0.)))?;
    assert_eq!(width(&mut ui), Some(INSPECTOR_MAX));
    ui.drag("resource-inspector-resize", point(px(2000.), px(0.)))?;
    assert_eq!(width(&mut ui), Some(INSPECTOR_MIN));
    ui.double_click("resource-inspector-resize")?;
    assert_eq!(width(&mut ui), Some(original));
    Ok(())
}

#[test]
fn events_namespace_choice_is_kept_per_cluster_in_preferences() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Events)?;
    let cluster = ui
        .state()
        .cluster
        .ok_or_else(|| anyhow::anyhow!("cluster"))?;
    ui.click("events-namespace")?;
    ui.click("events-namespace-option-0")?;
    let state = ui.state();
    let preferences = &state
        .snapshot
        .catalog
        .get(&cluster)
        .ok_or_else(|| anyhow::anyhow!("settings"))?
        .preferences;
    assert_eq!(
        preferences.namespaces.get(EVENT_LIST),
        Some(&fleet_core::settings::NamespaceSelection::All)
    );
    ui.press("cmd-shift-c")?;
    ui.type_text("staging")?;
    ui.press("enter")?;
    let other = ui.state();
    let other_preferences = &other
        .snapshot
        .catalog
        .get(&ClusterId::new("staging"))
        .ok_or_else(|| anyhow::anyhow!("other settings"))?
        .preferences;
    assert_eq!(other_preferences.namespaces.get(EVENT_LIST), None);
    ui.press("cmd-shift-c")?;
    ui.type_text(cluster.as_str())?;
    ui.press("enter")?;
    let restored = ui.state();
    assert_eq!(
        restored
            .snapshot
            .catalog
            .get(&cluster)
            .and_then(|settings| settings.preferences.namespaces.get(EVENT_LIST)),
        Some(&fleet_core::settings::NamespaceSelection::All)
    );
    Ok(())
}

#[test]
fn yaml_wrap_and_expand_work_and_escape_returns() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::ResourceInspector)?;
    assert!(ui.bounds("code-horizontal").is_some());
    ui.click("code-wrap")?;
    assert!(ui.bounds("code-horizontal").is_none());
    assert!(ui.bounds("code-wrapped").is_some());
    ui.click("code-expand")?;
    let expanded = ui
        .bounds("expanded-yaml")
        .ok_or_else(|| anyhow::anyhow!("no expanded YAML"))?;
    let inspector = ui
        .bounds("resource-inspector-pane")
        .ok_or_else(|| anyhow::anyhow!("no inspector"))?;
    assert!(expanded.size.width > inspector.size.width);
    ui.press("escape")?;
    assert!(ui.bounds("expanded-yaml").is_none());
    assert!(ui.bounds("code-wrapped").is_some());
    ui.click("code-wrap")?;
    assert!(ui.bounds("code-horizontal").is_some());
    Ok(())
}

fn picked(ui: &Ui, cluster: &str, list: &str) -> Option<fleet_core::settings::NamespaceSelection> {
    ui.state()
        .snapshot
        .catalog
        .get(&ClusterId::new(cluster))
        .and_then(|settings| settings.preferences.namespaces.get(list).cloned())
}

fn open_deployments(ui: &mut Ui, cluster: &str) -> anyhow::Result<()> {
    ui.press("cmd-shift-c")?;
    ui.type_text(cluster)?;
    ui.press("enter")?;
    ui.press("cmd-k")?;
    ui.type_text("deployments")?;
    ui.press("enter")?;
    assert_eq!(ui.state().page, Page::Deployments);
    Ok(())
}

#[test]
fn yaml_scrolls_in_both_axes_and_the_gutter_stays_fixed_horizontally() -> anyhow::Result<()> {
    use gpui::{point, px};
    let mut ui = Ui::open(Screen::ResourceInspector)?;
    let gutter = ui
        .bounds("code-gutter")
        .ok_or_else(|| anyhow::anyhow!("no gutter"))?;
    let lines = ui
        .bounds("code-lines")
        .ok_or_else(|| anyhow::anyhow!("no code"))?;
    ui.scroll("code-horizontal", point(px(-120.), px(0.)))?;
    let scrolled = ui
        .bounds("code-lines")
        .ok_or_else(|| anyhow::anyhow!("no code"))?;
    assert!(scrolled.origin.x < lines.origin.x);
    assert_eq!(ui.bounds("code-gutter"), Some(gutter));
    ui.scroll("code-vertical", point(px(0.), px(-120.)))?;
    let scrolled_gutter = ui
        .bounds("code-gutter")
        .ok_or_else(|| anyhow::anyhow!("no gutter"))?;
    assert!(scrolled_gutter.origin.y < gutter.origin.y);
    assert_eq!(scrolled_gutter.origin.x, gutter.origin.x);
    Ok(())
}

#[test]
fn overview_sections_navigate_to_their_lists() -> anyhow::Result<()> {
    for (selector, page) in [
        ("overview-pods", Page::Pods),
        ("overview-nodes", Page::Nodes),
        ("overview-deployments", Page::Deployments),
        ("overview-statefulsets", Page::StatefulSets),
        ("overview-daemonsets", Page::DaemonSets),
        ("overview-jobs", Page::Jobs),
        ("overview-cronjobs", Page::CronJobs),
    ] {
        let mut ui = Ui::open(Screen::Cluster)?;
        let cluster = ui.state().cluster;
        ui.click(selector)?;
        assert_eq!(ui.state().page, page);
        assert_eq!(ui.state().cluster, cluster);
    }
    Ok(())
}

#[test]
fn overview_sections_accept_keyboard_activation() -> anyhow::Result<()> {
    for key in ["enter", "space"] {
        let mut ui = Ui::open(Screen::Cluster)?;
        ui.press("cmd-2")?;
        ui.press(key)?;
        assert_eq!(ui.state().page, Page::Pods);
    }
    Ok(())
}

#[test]
fn selected_nav_rows_fill_the_panel_between_the_design_insets() -> anyhow::Result<()> {
    use fleet_ui::theme::{ROW_NAV, SIDEBAR, SPACE_4};

    let mut ui = Ui::open(Screen::Cluster)?;
    let overview = ui
        .bounds("nav-overview")
        .ok_or_else(|| anyhow::anyhow!("missing overview row"))?;
    ui.click("nav-nodes")?;
    let nodes = ui
        .bounds("nav-nodes")
        .ok_or_else(|| anyhow::anyhow!("missing nodes row"))?;
    assert_eq!(nodes.origin.x, overview.origin.x);
    assert_eq!(nodes.size.width, SIDEBAR - SPACE_4 - SPACE_4);
    assert_eq!(nodes.size.width, overview.size.width);
    assert_eq!(nodes.size.height, ROW_NAV);
    Ok(())
}

#[test]
fn node_inspector_shows_cpu_and_memory_commitments() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Nodes)?;
    ui.click("nodes-row-0")?;
    assert!(ui.bounds("node-capacity-CPU").is_some());
    assert!(ui.bounds("node-capacity-Memory").is_some());
    Ok(())
}

#[test]
fn the_terminal_tab_waits_for_an_open_shell_click() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Workload)?;
    ui.click("dock-tab-2")?;
    assert!(ui.bounds("workload-open-shell").is_some());
    assert!(ui.bounds("workload-close-shell").is_none());
    let mut pods = Ui::open(Screen::PodsShell)?;
    assert!(pods.bounds("pod-open-shell").is_some());
    assert!(pods.bounds("pod-close-shell").is_none());
    Ok(())
}

#[test]
fn keys_typed_in_an_open_shell_reach_it_including_tab_arrows_and_control_c() -> anyhow::Result<()> {
    for (screen, open, close) in [
        (
            Screen::Workload,
            "workload-open-shell",
            "workload-close-shell",
        ),
        (Screen::PodsShell, "pod-open-shell", "pod-close-shell"),
    ] {
        let mut ui = Ui::open(screen)?;
        if let Screen::Workload = screen {
            ui.click("dock-tab-2")?;
        }
        ui.click(open)?;
        assert!(ui.bounds(close).is_some());
        ui.type_text("whoami")?;
        ui.press("tab left up ctrl-c")?;
        ui.type_text("exit")?;
        ui.press("enter")?;
        assert!(ui.bounds(open).is_some(), "{open}: the shell did not exit");
        assert!(ui.bounds(close).is_none());
    }
    Ok(())
}

#[test]
fn close_shell_and_collapsing_the_dock_end_the_session_but_switching_tabs_keeps_it()
-> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Workload)?;
    ui.click("dock-tab-2")?;
    ui.click("workload-open-shell")?;
    ui.click("workload-close-shell")?;
    assert!(ui.bounds("workload-open-shell").is_some());
    ui.click("workload-open-shell")?;
    ui.click("dock-tab-0")?;
    ui.click("dock-tab-2")?;
    assert!(ui.bounds("workload-close-shell").is_some());
    ui.click("dock-toggle")?;
    ui.click("dock-toggle")?;
    assert!(ui.bounds("workload-open-shell").is_some());
    ui.click("workload-open-shell")?;
    ui.click("terminal-containers-1")?;
    assert!(ui.bounds("workload-open-shell").is_some());
    Ok(())
}

#[test]
fn pod_logs_have_no_previous_container_toggle() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::PodsShell)?;
    ui.click("dock-tab-0")?;
    assert!(ui.bounds("pod-logs-follow").is_some());
    assert!(ui.bounds("pod-logs-previous").is_none());
    Ok(())
}

#[test]
fn lists_start_in_the_default_namespace_and_remember_the_pick_per_cluster_and_kind()
-> anyhow::Result<()> {
    const BEYOND_ONE_NAMESPACE: &str = "resources-Deployment-row-8";
    let mut ui = Ui::open(Screen::Resources)?;
    assert_eq!(picked(&ui, SAMPLE, "Deployment"), None);
    assert!(ui.bounds(BEYOND_ONE_NAMESPACE).is_none());
    ui.click("resource-namespace")?;
    ui.click("resource-namespace-option-0")?;
    assert_eq!(
        picked(&ui, SAMPLE, "Deployment"),
        Some(fleet_core::settings::NamespaceSelection::All)
    );
    assert_eq!(picked(&ui, SAMPLE, EVENT_LIST), None);
    assert!(ui.bounds(BEYOND_ONE_NAMESPACE).is_some());
    open_deployments(&mut ui, "staging")?;
    assert_eq!(picked(&ui, "staging", "Deployment"), None);
    assert!(ui.bounds(BEYOND_ONE_NAMESPACE).is_none());
    open_deployments(&mut ui, SAMPLE)?;
    assert!(ui.bounds(BEYOND_ONE_NAMESPACE).is_some());
    Ok(())
}
