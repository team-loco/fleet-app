use std::rc::Rc;

use fleet_core::AppSettings;
use gpui::{App, Global};

use crate::theme;

pub type Saver = Rc<dyn Fn(&AppSettings) -> Result<(), String>>;

pub struct Settings {
    current: AppSettings,
    save: Option<Saver>,
    bundled: bool,
}

impl Global for Settings {}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(Settings {
        current: AppSettings::default(),
        save: None,
        bundled: false,
    });
}

pub fn install(settings: AppSettings, save: Saver, bundled: bool, cx: &mut App) {
    let choice = settings.theme;
    cx.set_global(Settings {
        current: settings,
        save: Some(save),
        bundled,
    });
    theme::set_choice(choice, cx);
}

pub fn current(cx: &App) -> &AppSettings {
    &cx.global::<Settings>().current
}

pub fn bundled(cx: &App) -> bool {
    cx.global::<Settings>().bundled
}

pub fn update(cx: &mut App, change: impl FnOnce(&mut AppSettings)) -> Result<(), String> {
    let before = current(cx).clone();
    let mut next = before.clone();
    change(&mut next);
    if next == before {
        return Ok(());
    }
    let save = cx.global::<Settings>().save.clone();
    let theme = next.theme;
    cx.global_mut::<Settings>().current = next.clone();
    if theme != before.theme {
        theme::set_choice(theme, cx);
    }
    save.map_or(Ok(()), |save| save(&next))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use fleet_core::app_settings::{PollInterval, ThemeChoice};
    use gpui::TestAppContext;

    use super::*;
    use crate::theme::{ActiveTheme, Appearance, Palette};

    #[gpui::test]
    fn changes_are_saved_once_and_the_theme_applies_at_once(cx: &mut TestAppContext) {
        let saved = Rc::new(RefCell::new(Vec::new()));
        let log = saved.clone();
        cx.update(|cx| {
            crate::init(None, cx).unwrap_or_else(|error| panic!("UI init: {error}"));
            let save: Saver = Rc::new(move |settings: &AppSettings| {
                log.borrow_mut().push(settings.clone());
                Ok(())
            });
            install(AppSettings::default(), save, false, cx);
            let result = update(cx, |s| s.theme = ThemeChoice::Light);
            assert_eq!(result, Ok(()));
            assert_eq!(cx.palette(), &Palette::for_appearance(Appearance::Light));
            let unchanged = update(cx, |s| s.theme = ThemeChoice::Light);
            assert_eq!(unchanged, Ok(()));
            let result = update(cx, |s| s.refresh.metrics = PollInterval::Seconds5);
            assert_eq!(result, Ok(()));
            assert_eq!(current(cx).refresh.metrics, PollInterval::Seconds5);
            assert_eq!(cx.palette(), &Palette::for_appearance(Appearance::Light));
            assert_eq!(update(cx, |s| s.theme = ThemeChoice::Dark), Ok(()));
            assert_eq!(cx.palette(), &Palette::for_appearance(Appearance::Dark));
        });
        let saved = saved.borrow();
        assert_eq!(saved.len(), 3);
        assert_eq!(saved[0].theme, ThemeChoice::Light);
        assert_eq!(saved[1].refresh.metrics, PollInterval::Seconds5);
    }
}
