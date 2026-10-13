use fleet::Screen;
use fleet_core::{ClusterId, Command, Page};
use fleet_shots::Ui;

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
fn command_b_and_command_shift_b_collapse_the_panels() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    let before = ui.state();
    assert!(before.clusters_open && before.resources_open);
    ui.press("cmd-b")?;
    assert!(!ui.state().resources_open);
    assert!(ui.state().clusters_open);
    ui.press("cmd-shift-b")?;
    assert!(!ui.state().clusters_open);
    ui.press("cmd-b cmd-shift-b")?;
    let after = ui.state();
    assert!(after.clusters_open && after.resources_open);
    Ok(())
}

#[test]
fn clicking_a_collapse_button_collapses_its_panel() -> anyhow::Result<()> {
    let mut ui = Ui::open(Screen::Cluster)?;
    ui.click("collapse-clusters")?;
    assert!(!ui.state().clusters_open);
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
