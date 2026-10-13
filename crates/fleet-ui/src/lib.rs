pub mod assets;
mod gallery;
pub mod kit;
pub mod screens;
mod shell;
pub mod theme;

pub use gallery::{Gallery, GalleryPage};
pub use gpui_component::Root;
pub use shell::Shell;

pub mod actions {
    gpui::actions!(
        fleet,
        [
            ToggleClustersPanel,
            ToggleResourcesPanel,
            ToggleCommandPalette
        ]
    );
}

pub fn init(
    forced: Option<theme::Appearance>,
    cx: &mut gpui::App,
) -> Result<(), assets::AssetError> {
    gpui_component::init(cx);
    assets::register_fonts(cx)?;
    theme::init(forced, cx);
    kit::init(cx);
    Ok(())
}
