use fleet_core::Status;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Window, div, px,
};

use crate::kit::{
    ActionMenu, Button, Checkbox, Column, ConfirmDialog, IconButton, Kbd, KeyValue, Link, MenuItem,
    PageHeader, Rule, SectionHeader, Segmented, Select, StatusGlyph, StatusText, Table, TextField,
    TextFieldState, Toast, Toggle, confirm, overlay_layer, toast,
};
use crate::theme::{
    self, ActiveTheme, FONT_MONO, Palette, RULE, SPACE_4, SPACE_6, SPACE_8, STRIP, TEXT_MONO,
    TEXT_UI,
};

const SELECT_WIDTH: Pixels = px(220.);
const FIELD_WIDTH: Pixels = px(220.);
const INSPECTOR_WIDTH: Pixels = px(360.);
const NAME_WEIGHT: f32 = 1.5;
const NAME_MIN: Pixels = px(210.);
const READY_WIDTH: Pixels = px(44.);
const STATUS_WIDTH: Pixels = px(150.);
const RESTARTS_WIDTH: Pixels = px(58.);
const NODE_WIDTH: Pixels = px(178.);
const CPU_WIDTH: Pixels = px(52.);
const MEM_WIDTH: Pixels = px(60.);
const AGE_WIDTH: Pixels = px(34.);

const STATUSES: [(Status, &str); 6] = [
    (Status::Ok, "Ready"),
    (Status::Warn, "Degraded"),
    (Status::Err, "CrashLoopBackOff"),
    (Status::Progress, "Progressing"),
    (Status::Suspended, "Suspended"),
    (Status::Offline, "Offline"),
];

#[derive(Clone)]
struct Pod {
    name: &'static str,
    ready: &'static str,
    status: Status,
    phase: &'static str,
    restarts: u32,
    node: &'static str,
    cpu: &'static str,
    mem: &'static str,
    age: &'static str,
}

const PODS: [Pod; 4] = [
    Pod {
        name: "checkout-api-7f9c6d-x2lqp",
        ready: "1/2",
        status: Status::Err,
        phase: "CrashLoopBackOff",
        restarts: 14,
        node: "ip-10-4-12-87",
        cpu: "4m",
        mem: "38Mi",
        age: "3h",
    },
    Pod {
        name: "checkout-api-7f9c6d-m8vtn",
        ready: "2/2",
        status: Status::Ok,
        phase: "Running",
        restarts: 0,
        node: "ip-10-4-14-201",
        cpu: "212m",
        mem: "301Mi",
        age: "3h",
    },
    Pod {
        name: "checkout-api-7f9c6d-q4zhk",
        ready: "2/2",
        status: Status::Ok,
        phase: "Running",
        restarts: 0,
        node: "ip-10-4-22-63",
        cpu: "187m",
        mem: "288Mi",
        age: "3h",
    },
    Pod {
        name: "checkout-api-7f9c6d-r7wbn",
        ready: "1/2",
        status: Status::Warn,
        phase: "Readiness failing",
        restarts: 2,
        node: "ip-10-4-22-63",
        cpu: "96m",
        mem: "120Mi",
        age: "12m",
    },
];

pub struct Gallery {
    more: Entity<ActionMenu>,
    actions: Entity<ActionMenu>,
    select: Entity<Select>,
    pods: Entity<Table<Pod>>,
    empty: Entity<Table<Pod>>,
    path: Entity<TextFieldState>,
    filter: Entity<TextFieldState>,
    follow: bool,
    notify: bool,
    include: bool,
    severity: usize,
    focus: FocusHandle,
    _appearance: Subscription,
}

impl Gallery {
    pub fn new(open_overlays: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let more = cx.new(|cx| ActionMenu::new("more", cx).items(more_items()));
        let actions = cx.new(|cx| {
            ActionMenu::new("actions", cx)
                .labeled("Actions", Some("⌘.".into()))
                .items(workload_items())
        });
        let select = cx.new(|cx| {
            Select::new(
                "context",
                vec![
                    "prod-us-east".into(),
                    "prod-eu-west".into(),
                    "staging".into(),
                ],
                cx,
            )
            .mono()
            .width(SELECT_WIDTH)
        });
        let pods = cx.new(|cx| {
            Table::new("pods", pod_columns(), cx)
                .rows(PODS.to_vec())
                .row_status(|pod: &Pod| (pod.status != Status::Ok).then_some(pod.status))
                .row_key(|pod: &Pod| pod.name.into())
                .fit_content()
        });
        let empty = cx.new(|cx| {
            Table::new("empty", pod_columns(), cx)
                .empty("No pods match the filter.")
                .fit_content()
        });
        let path = cx.new(|cx| TextFieldState::new(window, cx).default_value("apps/payments"));
        let filter = cx.new(|cx| TextFieldState::new(window, cx).placeholder("filter  /regex/"));
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            theme::follow_window(window, cx);
            cx.notify();
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        theme::follow_window(window, cx);
        if open_overlays {
            open_samples(&more, window, cx);
        }
        Self {
            more,
            actions,
            select,
            pods,
            empty,
            path,
            filter,
            follow: true,
            notify: true,
            include: true,
            severity: 0,
            focus,
            _appearance: appearance,
        }
    }

    fn statuses(palette: &Palette) -> impl IntoElement {
        let glyphs = STATUSES.map(|(status, _)| StatusGlyph::new(status));
        let texts = STATUSES.map(|(status, label)| StatusText::new(status, label));
        div()
            .flex()
            .flex_col()
            .gap(SPACE_4)
            .px(SPACE_8)
            .child(div().flex().gap(SPACE_6).children(glyphs))
            .child(
                div()
                    .flex()
                    .gap(SPACE_8)
                    .text_color(palette.fg)
                    .children(texts),
            )
    }

    fn header(&self) -> impl IntoElement {
        PageHeader::new("apps-payments")
            .mono()
            .status(Status::Err, "Health check failed")
            .action(
                Button::new("reconcile", "Reconcile")
                    .primary()
                    .shortcut("⌘R")
                    .on_click(|_, _, cx| toast(Toast::new(Status::Progress, "Reconciling apps-payments"), cx)),
            )
            .action(self.more.clone())
            .sentence(
                "One Deployment is not ready: checkout-api is crash-looping, so health checks time out after 5m.",
            )
            .meta(
                div()
                    .flex()
                    .gap(SPACE_4)
                    .child("applied")
                    .child(Link::new("applied", "a41e07c").mono())
                    .child("3h ago · last healthy")
                    .child(Link::new("healthy", "7c2d9e0").mono())
                    .child("1d ago"),
            )
    }

    fn buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(SPACE_4)
            .px(SPACE_8)
            .child(
                Button::new("b-primary", "Reconcile")
                    .primary()
                    .shortcut("⌘R"),
            )
            .child(Button::new("b-secondary", "Cancel"))
            .child(Button::new("b-ghost", "Paste kubeconfig…").ghost())
            .child(Button::new("b-danger", "Delete pod").danger())
            .child(Button::new("b-disabled", "Disabled").disabled(true))
            .child(
                Button::new("b-follow", "follow")
                    .small()
                    .selected(self.follow)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.follow = !this.follow;
                        cx.notify();
                    })),
            )
            .child(IconButton::glyph("b-close", "×", "Close"))
            .child(self.actions.clone())
            .child(self.select.clone())
            .child(Kbd::new("⌘K"))
            .child(Link::new("b-link", "Compare rendered output →"))
    }

    fn controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(SPACE_8)
            .px(SPACE_8)
            .child(
                TextField::new(&self.path)
                    .prefix("path:")
                    .width(FIELD_WIDTH),
            )
            .child(TextField::search(&self.filter))
            .child(
                Toggle::new("t-notify", self.notify)
                    .label("Confirm destructive actions")
                    .on_toggle(cx.listener(|this, on: &bool, _, cx| {
                        this.notify = *on;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("c-include", self.include)
                    .label("prod-us-east")
                    .on_toggle(cx.listener(|this, on: &bool, _, cx| {
                        this.include = *on;
                        cx.notify();
                    })),
            )
            .child(
                Segmented::new("severity")
                    .option("All")
                    .option("Errors")
                    .option("Warnings")
                    .selected(self.severity)
                    .on_select(cx.listener(|this, ix: &usize, _, cx| {
                        this.severity = *ix;
                        cx.notify();
                    })),
            )
            .child(
                Segmented::new("container")
                    .option("api")
                    .option("envoy")
                    .small()
                    .mono(),
            )
    }

    fn inspector(palette: &Palette) -> impl IntoElement {
        div().w(INSPECTOR_WIDTH).px(SPACE_8).child(
            KeyValue::new()
                .row("image", "ghcr.io/acme/checkout-api:2.14.0")
                .row("ports", "8080/TCP http")
                .row("requests", "cpu 500m · mem 512Mi")
                .row("limits", "cpu 1 · mem 1Gi")
                .row(
                    "last exit",
                    div().text_color(palette.err).child("1 (Error) · 2m ago"),
                ),
        )
    }

    fn feedback(cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap(SPACE_4)
            .px(SPACE_8)
            .child(Button::new("toast-ok", "Toast").on_click(|_, _, cx| {
                toast(
                    Toast::new(Status::Ok, "Scaled checkout-api to 3")
                        .detail("prod-us-east · payments"),
                    cx,
                );
            }))
            .child(
                Button::new("toast-err", "Error toast").on_click(|_, _, cx| {
                    toast(
                        Toast::new(Status::Err, "Reconcile failed")
                            .detail("kustomize build: accumulating resources"),
                        cx,
                    );
                }),
            )
            .child(
                Button::new("open-confirm", "Confirm dialog")
                    .danger()
                    .on_click(cx.listener(|_, _, window, cx| {
                        confirm(delete_dialog(), window, cx);
                    })),
            )
    }
}

fn open_samples(more: &Entity<ActionMenu>, window: &mut Window, cx: &mut App) {
    more.update(cx, |menu, cx| menu.show(window, cx));
    confirm(delete_dialog(), window, cx);
    toast(
        Toast::new(Status::Ok, "Scaled checkout-api to 3").detail("prod-us-east · payments"),
        cx,
    );
}

fn delete_dialog() -> ConfirmDialog {
    ConfirmDialog::new(
        "Delete pod",
        "The pod is deleted now. Its ReplicaSet creates a replacement within seconds; logs from this container are lost.",
        "Delete pod",
        |_, cx| toast(Toast::new(Status::Ok, "Deleted checkout-api-7f9c6d-x2lqp"), cx),
    )
    .detail("prod-us-east · payments/checkout-api-7f9c6d-x2lqp")
}

fn more_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("Suspend", |_, _| {})
            .shortcut("⌘⇧S")
            .description("Stop reconciling until resumed. Drift stays in place."),
        MenuItem::new("Reconcile with source", |_, _| {})
            .description("Fetch fleet-infra first, then apply."),
        MenuItem::new("View YAML", |_, _| {}).shortcut("⌘Y"),
        MenuItem::new("Delete…", |window, cx| {
            confirm(delete_dialog(), window, cx);
        })
        .destructive()
        .description("Prune every object this Kustomization manages."),
    ]
}

fn workload_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("Logs", |_, _| {}).shortcut("⌘L"),
        MenuItem::new("Shell into pod", |_, _| {}).shortcut("⌘T"),
        MenuItem::new("Restart rollout", |_, _| {}),
        MenuItem::new("Scale…", |_, _| {}).disabled(true),
        MenuItem::new("Roll back to revision 36", |_, _| {}).description(
            "Flux re-applies git on the next reconcile unless apps-payments is suspended.",
        ),
    ]
}

fn pod_columns() -> Vec<Column<Pod>> {
    vec![
        Column::text("Name", |pod: &Pod| pod.name.into())
            .mono()
            .flex(NAME_WEIGHT, NAME_MIN)
            .sort_by_key(|pod: &Pod| pod.name),
        Column::colored_text(
            "Ready",
            |pod: &Pod| pod.ready.into(),
            |pod, palette| palette.status(pod.status),
        )
        .mono()
        .fixed(READY_WIDTH),
        Column::new("Status", |pod: &Pod, _| {
            StatusText::new(pod.status, pod.phase).into_any_element()
        })
        .mono()
        .fixed(STATUS_WIDTH)
        .sort_by_key(|pod: &Pod| pod.status),
        Column::colored_text(
            "Restarts",
            |pod: &Pod| pod.restarts.to_string().into(),
            |pod, palette| {
                if pod.restarts > 0 {
                    palette.err
                } else {
                    palette.mute
                }
            },
        )
        .mono()
        .end()
        .fixed(RESTARTS_WIDTH)
        .sort_by_key(|pod: &Pod| pod.restarts),
        Column::colored_text("Node", |pod: &Pod| pod.node.into(), |_, p| p.fg2)
            .mono()
            .fixed(NODE_WIDTH),
        Column::colored_text("CPU", |pod: &Pod| pod.cpu.into(), |_, p| p.fg2)
            .mono()
            .end()
            .fixed(CPU_WIDTH),
        Column::colored_text("Mem", |pod: &Pod| pod.mem.into(), |_, p| p.fg2)
            .mono()
            .end()
            .fixed(MEM_WIDTH),
        Column::colored_text("Age", |pod: &Pod| pod.age.into(), |_, p| p.mute)
            .mono()
            .fixed(AGE_WIDTH),
    ]
}

fn section(label: &'static str, count: Option<&'static str>, body: impl IntoElement) -> AnyElement {
    let header = match count {
        Some(count) => SectionHeader::new(label).count(count),
        None => SectionHeader::new(label),
    };
    div()
        .flex()
        .flex_col()
        .gap(SPACE_4)
        .pb(SPACE_8)
        .child(header)
        .child(body)
        .child(div().pt(SPACE_8).child(Rule::horizontal()))
        .into_any_element()
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let note: SharedString =
            "Every primitive in fleet_ui::kit, drawn from theme tokens.".into();
        div()
            .id("gallery")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.editor)
            .text_color(palette.fg)
            .font_family(theme::FONT_SANS)
            .text_size(TEXT_UI)
            .child(
                div()
                    .h(STRIP)
                    .flex_none()
                    .bg(palette.chrome)
                    .border_b(RULE)
                    .border_color(palette.line),
            )
            .child(
                div()
                    .id("gallery-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.header())
                    .child(section(
                        "Status",
                        Some("shape and colour"),
                        Self::statuses(&palette),
                    ))
                    .child(section("Buttons", None, self.buttons(cx)))
                    .child(section("Controls", None, self.controls(cx)))
                    .child(section("Key / value", None, Self::inspector(&palette)))
                    .child(section(
                        "Pods",
                        Some("4 · click a header to sort"),
                        div()
                            .flex()
                            .flex_col()
                            .child(Rule::horizontal())
                            .child(self.pods.clone()),
                    ))
                    .child(section("Empty table", None, self.empty.clone()))
                    .child(section("Feedback", None, Self::feedback(cx)))
                    .child(
                        div()
                            .px(SPACE_8)
                            .pb(SPACE_8)
                            .font_family(FONT_MONO)
                            .text_size(TEXT_MONO)
                            .text_color(palette.mute)
                            .child(note),
                    ),
            )
            .child(overlay_layer(cx))
    }
}
