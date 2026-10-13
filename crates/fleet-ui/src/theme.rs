use std::time::Duration;

use fleet_core::Status;
use gpui::{App, Global, Hsla, Pixels, Window, WindowAppearance, px, rgb, rgba};

pub const FONT_SANS: &str = "IBM Plex Sans";
pub const FONT_MONO: &str = "IBM Plex Mono";

pub const TEXT_UI: Pixels = px(13.);
pub const TEXT_MONO: Pixels = px(12.);
pub const TEXT_LABEL: Pixels = px(11.);
pub const TEXT_BADGE: Pixels = px(11.5);
pub const TEXT_GLYPH: Pixels = px(10.);
pub const TEXT_TITLE: Pixels = px(14.);

pub const RADIUS: Pixels = px(3.);
pub const RADIUS_MENU: Pixels = px(4.);
pub const RULE: Pixels = px(1.);

pub const ROW_TABLE_MIN: Pixels = px(24.);
pub const ROW_TABLE_MAX: Pixels = px(28.);
pub const ROW_NAV: Pixels = px(26.);
pub const ROW_CLUSTER: Pixels = px(28.);

pub const STRIP: Pixels = px(28.);
pub const CLUSTERS_PANEL: Pixels = px(164.);
pub const RESOURCES_PANEL: Pixels = px(196.);
pub const NAV_MIN_WIDTH: Pixels = px(84.);
pub const PANEL_HEADER: Pixels = px(32.);
pub const PAGE_HEADER: Pixels = px(44.);
pub const STATUS_BAR: Pixels = px(26.);

pub const ICON: Pixels = px(11.);
pub const ICON_BUTTON: Pixels = px(22.);
pub const GLYPH_SLOT: Pixels = px(10.);
pub const TRAFFIC_LIGHT_X: Pixels = px(10.);
pub const TRAFFIC_LIGHT_Y: Pixels = px(8.);

pub const SPACE_1: Pixels = px(2.);
pub const SPACE_2: Pixels = px(4.);
pub const SPACE_3: Pixels = px(6.);
pub const SPACE_4: Pixels = px(8.);
pub const SPACE_5: Pixels = px(10.);
pub const SPACE_6: Pixels = px(12.);
pub const SPACE_7: Pixels = px(14.);
pub const SPACE_8: Pixels = px(16.);

pub const COLLAPSE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}

impl Appearance {
    pub fn from_window(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark => Self::Dark,
            WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light => Self::Light,
            WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub editor: Hsla,
    pub panel: Hsla,
    pub chrome: Hsla,
    pub hover: Hsla,
    pub line: Hsla,
    pub line2: Hsla,
    pub fg: Hsla,
    pub fg2: Hsla,
    pub mute: Hsla,
    pub sel: Hsla,
    pub selfg: Hsla,
    pub selline: Hsla,
    pub acc: Hsla,
    pub onacc: Hsla,
    pub ok: Hsla,
    pub warn: Hsla,
    pub err: Hsla,
    pub sus: Hsla,
    pub okbg: Hsla,
    pub warnbg: Hsla,
    pub errbg: Hsla,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            editor: rgb(0x0011_1317).into(),
            panel: rgb(0x0015_181D).into(),
            chrome: rgb(0x001A_1D22).into(),
            hover: rgb(0x001D_2127).into(),
            line: rgb(0x0026_2A30).into(),
            line2: rgb(0x0032_3740).into(),
            fg: rgb(0x00DC_DFE4).into(),
            fg2: rgb(0x00B3_B9C3).into(),
            mute: rgb(0x008E_96A3).into(),
            sel: rgb(0x001F_2C40).into(),
            selfg: rgb(0x00F2_F6FC).into(),
            selline: rgb(0x003A_5A8C).into(),
            acc: rgb(0x0074_ADFF).into(),
            onacc: rgb(0x000C_1220).into(),
            ok: rgb(0x0048_C78E).into(),
            warn: rgb(0x00E8_B04B).into(),
            err: rgb(0x00F2_6D6D).into(),
            sus: rgb(0x009A_A2AE).into(),
            okbg: rgba(0x48C7_8E1A).into(),
            warnbg: rgba(0xE8B0_4B1A).into(),
            errbg: rgba(0xF26D_6D1C).into(),
        }
    }

    pub fn light() -> Self {
        Self {
            editor: rgb(0x00FF_FFFF).into(),
            panel: rgb(0x00F6_F7F8).into(),
            chrome: rgb(0x00EE_EFF1).into(),
            hover: rgb(0x00F1_F2F4).into(),
            line: rgb(0x00E1_E3E7).into(),
            line2: rgb(0x00CF_D3D9).into(),
            fg: rgb(0x001D_2027).into(),
            fg2: rgb(0x003B_414B).into(),
            mute: rgb(0x005F_6774).into(),
            sel: rgb(0x00DC_E7FA).into(),
            selfg: rgb(0x000D_2850).into(),
            selline: rgb(0x008D_B0EA).into(),
            acc: rgb(0x001F_5FD1).into(),
            onacc: rgb(0x00FF_FFFF).into(),
            ok: rgb(0x0016_7A4C).into(),
            warn: rgb(0x0093_5800).into(),
            err: rgb(0x00C5_3030).into(),
            sus: rgb(0x006B_7280).into(),
            okbg: rgba(0x167A_4C14).into(),
            warnbg: rgba(0x9358_0014).into(),
            errbg: rgba(0xC530_3012).into(),
        }
    }

    pub fn for_appearance(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Dark => Self::dark(),
            Appearance::Light => Self::light(),
        }
    }

    pub fn status(&self, status: Status) -> Hsla {
        match status {
            Status::Ok => self.ok,
            Status::Warn => self.warn,
            Status::Err => self.err,
            Status::Progress => self.acc,
            Status::Suspended => self.sus,
            Status::Offline => self.mute,
        }
    }
}

pub struct Theme {
    pub forced: Option<Appearance>,
    pub appearance: Appearance,
    pub palette: Palette,
}

impl Global for Theme {}

pub trait ActiveTheme {
    fn palette(&self) -> &Palette;
}

impl ActiveTheme for App {
    fn palette(&self) -> &Palette {
        &self.global::<Theme>().palette
    }
}

pub fn init(forced: Option<Appearance>, cx: &mut App) {
    let appearance = forced.unwrap_or_else(|| Appearance::from_window(cx.window_appearance()));
    cx.set_global(Theme {
        forced,
        appearance,
        palette: Palette::for_appearance(appearance),
    });
    sync_component_theme(appearance, cx);
}

pub fn follow_window(window: &Window, cx: &mut App) {
    let theme = cx.global::<Theme>();
    let appearance = theme
        .forced
        .unwrap_or_else(|| Appearance::from_window(window.appearance()));
    if appearance == theme.appearance {
        return;
    }
    let theme = cx.global_mut::<Theme>();
    theme.appearance = appearance;
    theme.palette = Palette::for_appearance(appearance);
    sync_component_theme(appearance, cx);
}

fn sync_component_theme(appearance: Appearance, cx: &mut App) {
    let mode = match appearance {
        Appearance::Dark => gpui_component::ThemeMode::Dark,
        Appearance::Light => gpui_component::ThemeMode::Light,
    };
    gpui_component::Theme::change(mode, None, cx);
    let palette = Palette::for_appearance(appearance);
    gpui_component::Theme::update(cx, |theme| {
        theme.font_family = FONT_SANS.into();
        theme.mono_font_family = FONT_MONO.into();
        theme.font_size = TEXT_UI;
        theme.mono_font_size = TEXT_MONO;
        theme.radius = RADIUS;
        theme.radius_lg = RADIUS_MENU;
        theme.shadow = false;
        theme.background = palette.editor;
        theme.foreground = palette.fg;
        theme.muted_foreground = palette.mute;
        theme.border = palette.line;
        theme.input = palette.line2;
        theme.primary = palette.acc;
        theme.primary_foreground = palette.onacc;
        theme.accent = palette.hover;
        theme.selection = palette.sel;
        theme.popover = palette.panel;
        theme.popover_foreground = palette.fg;
        theme.danger = palette.err;
        theme.warning = palette.warn;
        theme.success = palette.ok;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_colours_use_status_tokens() {
        let palette = Palette::dark();
        assert_eq!(palette.status(Status::Err), palette.err);
        assert_eq!(palette.status(Status::Offline), palette.mute);
    }

    #[test]
    fn tinted_backgrounds_are_translucent() {
        for palette in [Palette::dark(), Palette::light()] {
            for tint in [palette.okbg, palette.warnbg, palette.errbg] {
                assert!(tint.a > 0.05 && tint.a < 0.15);
            }
        }
    }
}
