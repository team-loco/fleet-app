pub mod assets;
mod gallery;
pub mod kit;
pub mod prefs;
pub mod screens;
pub mod settings;
mod shell;
mod store;
pub mod theme;

pub use gallery::{Gallery, GalleryPage};
pub use gpui_component::Root;
pub use shell::{NewWindowRequest, Shell, key_bindings};
pub use store::Store;

pub mod actions {
    gpui::actions!(
        fleet,
        [
            ToggleSidebar,
            ToggleClusterSwitcher,
            ToggleCommandPalette,
            FollowLogs,
            CompareAcrossClusters,
            ShowFluxOwner,
            OpenSelectedNewWindow,
            CloseWindow,
            OpenSettings,
            FocusFilter
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
    settings::init(cx);
    kit::init(cx);
    screens::init(cx);
    Ok(())
}
