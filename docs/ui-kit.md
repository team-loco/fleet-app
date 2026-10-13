# UI kit

`fleet_ui::kit` holds the primitives every screen is built from. Use them instead of styling raw
`div`s; all colours, sizes and radii come from `fleet_ui::theme` (`cx.palette()` and the `theme::*`
constants). If you need a value the theme lacks, add a named constant there.

See every primitive running: `cargo run -p fleet -- --demo --screen kit` (or `kit-overlays` for
the open menu, confirm dialog and a toast), `--appearance dark|light`. Screenshots:
`mise run shot -- kit dark`. The gallery source (`crates/fleet-ui/src/gallery.rs`) is a working
usage example for everything below.

```rust
use fleet_ui::kit::*;
use fleet_ui::theme::{self, ActiveTheme};
```

Two kinds of primitive:

- **Elements** (`RenderOnce` builders): construct in `render` and pass to `.child(...)`. Stateless.
- **Views** (`Entity<T>`): create once with `cx.new(...)`, keep the entity in your view's struct,
  render it with `.child(entity.clone())`. They keep their own state (open, sorted, scrolled) and
  report back through `cx.subscribe` events.

Handlers have the shape `Fn(&Arg, &mut Window, &mut App)`, so `cx.listener(|this, arg, window, cx| ..)`
from your view fits directly.

## Status

| primitive | API |
|---|---|
| `StatusGlyph` | `StatusGlyph::new(Status::Err)`: the shape (● ▲ ✕ ◐ ‖ ○) in the status colour, 10px slot |
| `StatusText` | `StatusText::new(Status::Err, "CrashLoopBackOff")`: glyph + label, both in the status colour, truncates |

Row tints for a status: `palette.tint(status)` returns `okbg` / `warnbg` / `errbg` (or `None`).

## Text and layout

```rust
SectionHeader::new("Pods").count("3 · click to follow logs").end(Segmented::new("f")...)
SectionLabel::new("Container")            // uppercase 11px mute label alone
Rule::horizontal()  /  Rule::vertical()   // 1px `line`
Link::new("rev", "a41e07c").mono().on_click(|_, window, cx| ..)
Kbd::new("⌘K")
KeyValue::new()                           // inspector / spec grid, label column 72px muted
    .row("image", "ghcr.io/acme/checkout-api:2.14.0")
    .row("last exit", div().text_color(palette.err).child("1 (Error) · 2m ago"))
    .label_width(px)                      // optional; `.sans()` for proportional values
```

`PageHeader` is the document header of a resource page (title, status, one primary action, the
⋯ menu, the one-sentence summary, a mono meta line):

```rust
PageHeader::new("checkout-api")
    .mono()
    .status(Status::Err, "2/3 ready")
    .action(Button::new("reconcile", "Reconcile").primary().shortcut("⌘R").on_click(..))
    .action(self.menu.clone())                 // Entity<ActionMenu>
    .sentence("One pod is crash-looping, so the rollout to revision 37 is stuck.")
    .meta(div().flex().gap(SPACE_4).child("payments ·").child(Link::new("ks", "apps-payments")))
```

## Buttons and controls

```rust
Button::new("id", "Reconcile").primary().shortcut("⌘R")   // acc / onacc
Button::new("id", "Cancel")                               // secondary: line2 border (default)
Button::new("id", "Paste kubeconfig…").ghost()            // acc text, no border
Button::new("id", "Delete pod").danger()
Button::new("id", "follow").small().selected(self.follow) // dock toggles: sel/selfg/selline
    .disabled(bool) .tooltip("text") .on_click(|event, window, cx| ..)

IconButton::glyph("close", "×", "Close")                  // tooltip text is required
IconButton::svg("collapse", assets::CHEVRON_LEFT, "Collapse").bordered().selected(bool)

Segmented::new("severity").option("All").option("Errors").option("Warnings")
    .selected(ix).small().mono()
    .on_select(cx.listener(|this, ix: &usize, _, cx| ..))

Toggle::new("confirm", on).label("Confirm destructive actions")
    .on_toggle(cx.listener(|this, on: &bool, _, cx| ..))
Checkbox::new("ctx", checked).label("prod-us-east").on_toggle(..)
```

Tooltips on any stateful element: `.tooltip(Tooltip::text("Copy commit"))`.

### Text fields

Wraps gpui-component's input. Keep the state entity in your view:

```rust
let filter = cx.new(|cx| TextFieldState::new(window, cx).placeholder("filter  /regex/"));
cx.subscribe(&filter, |this, state, event: &InputEvent, cx| {
    if let InputEvent::Change = event { let q = state.read(cx).value(); .. }
}).detach();

TextField::new(&self.filter).prefix("path:").width(px)    // 24px, mono 12
TextField::search(&self.filter)                            // 20px x 180px, mono 11.5 (dock filters)
```

### Select

```rust
let select = cx.new(|cx| Select::new("context", vec!["prod-us-east".into(), ..], cx).mono().width(px));
cx.subscribe(&select, |this, _, SelectEvent::Changed(ix), cx| ..).detach();
select.read(cx).selected_index()
```

Own popover (same look and keyboard as the action menu); arrow keys, Enter, Esc.

## ActionMenu (⋯ / Actions)

Each item has a label and an optional one-line description, shortcut hint, destructive styling and
disabled state. Opens below the trigger, right-aligned, fades in; ↑ ↓ Enter, Esc or a click outside
closes it and returns focus.

```rust
let menu = cx.new(|cx| {
    ActionMenu::new("ks-actions", cx)                 // ⋯ trigger
        .items(vec![
            MenuItem::new("Suspend", |window, cx| ..)
                .shortcut("⌘⇧S")
                .description("Stop reconciling until resumed. Drift stays in place."),
            MenuItem::new("Delete…", |window, cx| confirm(dialog(), window, cx))
                .destructive()
                .description("Prune every object this Kustomization manages."),
        ])
});
ActionMenu::new("workload-actions", cx).labeled("Actions", Some("⌘.".into())) // labelled trigger
menu.update(cx, |m, cx| m.toggle(window, cx));   // e.g. from a ⌘. key binding
menu.update(cx, |m, cx| m.set_items(items, cx));
```

## ConfirmDialog and toasts

Both render in the window-wide overlay layer. Every root view must include it once:
`.child(kit::overlay_layer(cx))` (`Shell` and the gallery already do).

```rust
confirm(
    ConfirmDialog::new(
        "Delete pod",
        "The pod is deleted now. Its ReplicaSet creates a replacement within seconds; logs from this container are lost.",
        "Delete pod",                                  // danger button label
        |window, cx| { /* send the Command */ },
    )
    .detail("prod-us-east · payments/checkout-api-7f9c6d-x2lqp"),   // mono lines, optional
    window,
    cx,
);

toast(Toast::new(Status::Ok, "Scaled checkout-api to 3").detail("prod-us-east · payments"), cx);
```

The dialog is 440px, 4px radius, over a scrim; Esc or Cancel dismisses, ⌘↵ confirms. Toasts stack
bottom-right above the status bar and dismiss after `theme::TOAST_TIMEOUT` or on click.

## Table

Generic over your row type, virtualised with `uniform_list`, sticky header, hover, selection
(`sel` + `selline` outline), status-tinted rows, sortable headers (click cycles ↑ ↓ off), keyboard
↑ ↓ Enter.

```rust
let pods = cx.new(|cx| {
    Table::new("pods", vec![
        Column::text("Name", |p: &Pod| p.name.clone()).mono().flex(1.5, px(210.)).sort_by_key(|p: &Pod| p.name.clone()),
        Column::colored_text("Ready", |p: &Pod| p.ready.clone(), |p, palette| palette.status(p.status)).mono().fixed(px(44.)),
        Column::new("Status", |p: &Pod, _palette| StatusText::new(p.status, p.phase.clone()).into_any_element()).fixed(px(150.)),
        Column::text("Restarts", |p: &Pod| p.restarts.to_string().into()).mono().end().fixed(px(58.)),
    ], cx)
    .rows(pods)
    .row_status(|p: &Pod| (p.status != Status::Ok).then_some(p.status))  // okbg/warnbg/errbg tint
    .row_key(|p: &Pod| p.name.clone())   // keeps selection across updates, flashes rows whose status changed
    .empty("No pods match the filter.")
    .fit_content()                        // inline in a scrolling page; omit to fill the parent and scroll
});
cx.subscribe(&pods, |this, table, event: &TableEvent, cx| match event {
    TableEvent::Selected(ix) => ..,        // index into the rows you passed
    TableEvent::Opened(ix) => ..,          // double click or Enter
}).detach();
pods.update(cx, |t, cx| t.set_rows(new_rows, cx));
```

Column options: `.fixed(px)`, `.flex(weight, min)`, `.end()`, `.center()`, `.mono()` (Plex Mono 12;
rows are otherwise Sans 12.5), `.sortable(cmp)` / `.sort_by_key(key)`. Table options:
`.row_height(px)` (default `ROW_TABLE` 26), `.headerless()`, `.sorted_by(col, dir)`.
Column widths are design values; define them as `const`s at the top of your screen file.

## Relative time

`fleet_core::time::age(duration) -> "4s" | "2m" | "3h" | "9d" | "1y"` and
`age_since(then, now)` (future timestamps read as `0s`).
