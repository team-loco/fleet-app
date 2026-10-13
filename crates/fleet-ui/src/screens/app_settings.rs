use fleet_core::app_settings::{PollInterval, SnapshotCap, ThemeChoice};
use fleet_core::{AppSettings, Command, Page, Status};
use gpui::{
    AnyElement, App, Context, EventEmitter, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window, div, relative,
};

use crate::keymap;
use crate::kit::{Kbd, Link, SectionLabel, Segmented, Toast, Toggle, toast};
use crate::settings::{self, Settings};
use crate::theme::{
    ActiveTheme, LINE_PROSE, PAGE_HEADER, Palette, ROW_DOCUMENT, RULE, SETTINGS_COLUMN, SPACE_1,
    SPACE_2, SPACE_6, SPACE_7, SPACE_8, TEXT_SMALL, TEXT_TITLE,
};

const APPEARANCE: &str = "Appearance";
const THEME: &str = "Theme";
const THEME_HINT: &str = "System follows macOS as it switches between light and dark.";
const REFRESH: &str = "Data refresh";
const REFRESH_SENTENCE: &str =
    "Resource changes are pushed by Kubernetes watches as they happen; only metrics are polled.";
const METRICS_SERVER: &str = "metrics-server";
const METRICS_SERVER_HINT: &str = "Node and pod usage on every connected cluster.";
const PROMETHEUS: &str = "Prometheus";
const PROMETHEUS_HINT: &str = "Utilization history on cluster overviews.";
const SNAPSHOTS: &str = "Screen updates";
const SNAPSHOTS_HINT: &str = "Fleet redraws at most this often, however busy the clusters are.";
const NOTIFICATIONS: &str = "Notifications";
const NEW_PROBLEMS: &str = "New problems";
const NEW_PROBLEMS_HINT: &str =
    "A macOS notification when a resource starts failing. A flapping pod is announced once.";
const WARNINGS: &str = "Include warnings";
const WARNINGS_HINT: &str = "Also notify for warnings, not only errors.";
const MUTE_PREFIX: &str = "Mute a cluster from its settings in ";
const MUTE_SUFFIX: &str = ".";
const UNBUNDLED: &str =
    "Fleet is running outside Fleet.app, so macOS will not show notifications from this copy.";
const SAVE_FAILED: &str = "Settings could not be saved";
const SHORTCUTS: &str = "Keyboard shortcuts";

pub struct AppSettingsScreen {
    _settings: Subscription,
}

impl EventEmitter<Command> for AppSettingsScreen {}

impl AppSettingsScreen {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let observe = cx.observe_global::<Settings>(|_, cx| cx.notify());
        Self { _settings: observe }
    }

    fn header(palette: &Palette) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(PAGE_HEADER)
            .px(SPACE_8)
            .border_b(RULE)
            .border_color(palette.line)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .child(
                div()
                    .text_size(TEXT_TITLE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Page::Settings.title()),
            )
    }

    fn appearance(current: &AppSettings, palette: &Palette) -> impl IntoElement {
        let selected = ThemeChoice::ALL
            .iter()
            .position(|choice| *choice == current.theme)
            .unwrap_or_default();
        let choices = ThemeChoice::ALL
            .iter()
            .fold(Segmented::new("settings-theme"), |control, choice| {
                control.option(choice.label())
            })
            .selected(selected)
            .on_select(|ix, _, cx| {
                if let Some(choice) = ThemeChoice::ALL.get(*ix).copied() {
                    change(cx, |s| s.theme = choice);
                }
            });
        section(APPEARANCE, palette).child(row(THEME, THEME_HINT, choices, palette))
    }

    fn refresh(
        current: &AppSettings,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let refresh = current.refresh;
        let metrics = intervals("settings-metrics", refresh.metrics).on_select(cx.listener(
            |_, ix: &usize, _, cx| {
                if let Some(interval) = PollInterval::ALL.get(*ix).copied() {
                    Self::set_refresh(cx, |s| s.refresh.metrics = interval);
                }
            },
        ));
        let prometheus = intervals("settings-prometheus", refresh.prometheus).on_select(
            cx.listener(|_, ix: &usize, _, cx| {
                if let Some(interval) = PollInterval::ALL.get(*ix).copied() {
                    Self::set_refresh(cx, |s| s.refresh.prometheus = interval);
                }
            }),
        );
        let selected = SnapshotCap::ALL
            .iter()
            .position(|cap| *cap == refresh.snapshots)
            .unwrap_or_default();
        let snapshots = SnapshotCap::ALL
            .iter()
            .fold(Segmented::new("settings-snapshots"), |control, cap| {
                control.option(cap.label())
            })
            .mono()
            .selected(selected)
            .on_select(cx.listener(|_, ix: &usize, _, cx| {
                if let Some(cap) = SnapshotCap::ALL.get(*ix).copied() {
                    Self::set_refresh(cx, |s| s.refresh.snapshots = cap);
                }
            }));
        section(REFRESH, palette)
            .child(prose(REFRESH_SENTENCE, palette))
            .child(row(METRICS_SERVER, METRICS_SERVER_HINT, metrics, palette))
            .child(row(PROMETHEUS, PROMETHEUS_HINT, prometheus, palette))
            .child(row(SNAPSHOTS, SNAPSHOTS_HINT, snapshots, palette))
    }

    fn notifications(
        current: &AppSettings,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let notifications = current.notifications;
        let enabled =
            Toggle::new("settings-notify", notifications.enabled).on_toggle(|on, _, cx| {
                let on = *on;
                change(cx, |s| s.notifications.enabled = on);
            });
        let warnings =
            Toggle::new("settings-warnings", notifications.warnings).on_toggle(|on, _, cx| {
                let on = *on;
                change(cx, |s| s.notifications.warnings = on);
            });
        let clusters = Link::new("settings-clusters", Page::ClusterSettings.label()).on_click(
            cx.listener(|_, _, _, cx| cx.emit(Command::SelectPage(Page::ClusterSettings))),
        );
        let mute = div()
            .flex()
            .text_size(TEXT_SMALL)
            .text_color(palette.mute)
            .child(MUTE_PREFIX)
            .child(clusters)
            .child(MUTE_SUFFIX);
        let unbundled = (!settings::bundled(cx)).then(|| {
            div()
                .text_size(TEXT_SMALL)
                .text_color(palette.status(Status::Warn))
                .child(UNBUNDLED)
        });
        section(NOTIFICATIONS, palette)
            .child(row(NEW_PROBLEMS, NEW_PROBLEMS_HINT, enabled, palette))
            .child(row(WARNINGS, WARNINGS_HINT, warnings, palette))
            .child(mute)
            .children(unbundled)
    }

    fn set_refresh(cx: &mut Context<Self>, edit: impl FnOnce(&mut AppSettings)) {
        let before = settings::current(cx).refresh;
        change(cx, edit);
        let after = settings::current(cx).refresh;
        if after != before {
            cx.emit(Command::SetRefresh(after));
        }
    }
}

fn shortcuts(palette: &Palette) -> impl IntoElement {
    let groups = keymap::grouped().into_iter().map(|(group, bindings)| {
        let rows = bindings.into_iter().map(|binding| {
            let keys = binding
                .keys
                .iter()
                .map(|keys| Kbd::new(keymap::display(keys)));
            div()
                .flex()
                .items_center()
                .gap(SPACE_8)
                .h(ROW_DOCUMENT)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(palette.fg2)
                        .child(binding.description),
                )
                .child(div().flex().flex_none().gap(SPACE_2).children(keys))
        });
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .pb(SPACE_2)
                    .text_size(TEXT_SMALL)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.fg)
                    .child(group.label()),
            )
            .children(rows)
    });
    section(SHORTCUTS, palette).children(groups)
}

fn change(cx: &mut App, edit: impl FnOnce(&mut AppSettings)) {
    if let Err(error) = settings::update(cx, edit) {
        tracing::warn!(%error, "saving settings");
        toast(Toast::new(Status::Err, SAVE_FAILED).detail(error), cx);
    }
}

fn intervals(id: &'static str, current: PollInterval) -> Segmented {
    let selected = PollInterval::ALL
        .iter()
        .position(|interval| *interval == current)
        .unwrap_or_default();
    PollInterval::ALL
        .iter()
        .fold(Segmented::new(id), |control, interval| {
            control.option(interval.label())
        })
        .mono()
        .selected(selected)
}

fn section(label: &'static str, palette: &Palette) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(SPACE_6)
        .py(SPACE_7)
        .border_b(RULE)
        .border_color(palette.line)
        .child(SectionLabel::new(SharedString::from(label)))
}

fn prose(text: &'static str, palette: &Palette) -> impl IntoElement {
    div()
        .text_color(palette.fg2)
        .line_height(relative(LINE_PROSE))
        .child(text)
}

fn row(
    title: &'static str,
    hint: &'static str,
    control: impl IntoElement,
    palette: &Palette,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(SPACE_8)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(SPACE_1)
                .child(div().text_color(palette.fg).child(title))
                .child(
                    div()
                        .text_size(TEXT_SMALL)
                        .text_color(palette.mute)
                        .child(hint),
                ),
        )
        .child(div().flex().flex_none().child(control))
        .into_any_element()
}

impl Render for AppSettingsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let current = settings::current(cx).clone();
        let column = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(SETTINGS_COLUMN)
            .px(SPACE_8)
            .pb(SPACE_8)
            .child(Self::appearance(&current, &palette))
            .child(Self::refresh(&current, &palette, cx))
            .child(Self::notifications(&current, &palette, cx))
            .child(shortcuts(&palette));
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(Self::header(&palette))
            .child(
                div()
                    .id("app-settings")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(column),
            )
    }
}
