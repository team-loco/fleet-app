use std::borrow::Cow;

use gpui::{App, AssetSource, SharedString};

pub const CHEVRON_LEFT: &str = "icons/chevron-left.svg";
pub const CHEVRON_RIGHT: &str = "icons/chevron-right.svg";
pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
pub const CHEVRON_UP: &str = "icons/chevron-up.svg";
pub const CHECK: &str = "icons/check.svg";

const ICONS: &[(&str, &[u8])] = &[
    (
        CHEVRON_LEFT,
        include_bytes!("../assets/icons/chevron-left.svg"),
    ),
    (
        CHEVRON_RIGHT,
        include_bytes!("../assets/icons/chevron-right.svg"),
    ),
    (
        CHEVRON_DOWN,
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
    (CHEVRON_UP, include_bytes!("../assets/icons/chevron-up.svg")),
    (CHECK, include_bytes!("../assets/icons/check.svg")),
];

pub(crate) const PLEX_SANS_SEMIBOLD: &[u8] =
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");

const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"),
    PLEX_SANS_SEMIBOLD,
    include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"),
];

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("registering bundled fonts: {0}")]
    Fonts(String),
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let found = ICONS.iter().find(|(name, _)| *name == path);
        Ok(found.map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let names = ICONS.iter().map(|(name, _)| *name);
        Ok(names
            .filter(|name| name.starts_with(path))
            .map(SharedString::new_static)
            .collect())
    }
}

pub fn register_fonts(cx: &App) -> Result<(), AssetError> {
    let fonts = FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect();
    cx.text_system()
        .add_fonts(fonts)
        .map_err(|error| AssetError::Fonts(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_bundled_icons_only() {
        let left = Assets.load(CHEVRON_LEFT).ok().flatten();
        assert!(left.is_some_and(|bytes| bytes.starts_with(b"<svg")));
        assert!(Assets.load("icons/missing.svg").ok().flatten().is_none());
    }
}
