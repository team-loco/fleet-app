pub mod assets;
mod shell;
pub mod theme;

pub use gpui_component::Root;
pub use shell::Shell;

pub mod actions {
    gpui::actions!(fleet, [ToggleClustersPanel, ToggleResourcesPanel]);
}

pub fn init(
    forced: Option<theme::Appearance>,
    cx: &mut gpui::App,
) -> Result<(), assets::AssetError> {
    gpui_component::init(cx);
    assets::register_fonts(cx)?;
    theme::init(forced, cx);
    Ok(())
}
