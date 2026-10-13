# gpui notes

What P0 learned about `gpui-pre` 0.3.9 and `gpui-component` 0.7.3. Sources live in
`~/.cargo/registry/src/*/gpui-pre-0.3.9` (examples in `examples/`) and `gpui-component-0.7.3/src`.

## Crates

- `gpui` = `gpui-pre` (renamed in `[workspace.dependencies]`).
- `gpui-platform` = `gpui-pre-platform`. `gpui-pre` has no platform of its own; the app is started with
  `gpui_platform::application()` (returns `gpui::Application`). Only `apps/fleet` depends on it.
- Features on `gpui-pre-platform`: `runtime_shaders` is required unless full Xcode is installed (the
  build script otherwise runs `xcrun metal` and fails with "metal shader compilation failed").
  `font-kit` matches what upstream uses for its own tests.
- `gpui-component` (the styled kit) re-exports `gpui_base::Root`; `fleet_ui::Root` re-exports it again so
  the app never names `gpui-component` directly. The umbrella `gpui-kit` crate exists but is not used.

## Startup

```rust
gpui_platform::application()
    .with_assets(fleet_ui::assets::Assets)
    .run(|cx: &mut App| {
        fleet_ui::init(forced_appearance, cx)?; // gpui_component::init + fonts + theme
        cx.bind_keys([KeyBinding::new("cmd-b", ToggleResourcesPanel, None)]);
        cx.open_window(options, |window, cx| {
            let shell = cx.new(|cx| Shell::new(updates, commands, initial, window, cx));
            cx.new(|cx| Root::new(shell, window, cx))
        })?;
    });
```

- `gpui_component::init(cx)` must run before any component and before `Theme::change`.
- Every window's root view must be `Root` (overlays, menus, dialogs, notifications hang off it).
- `WindowOptions.titlebar = TitlebarOptions { title: None, appears_transparent: true,
  traffic_light_position: Some(point(px(10.), px(8.))) }` gives the hidden titlebar with traffic lights
  in our 28px strip. Dragging: `on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())`
  on the strip and page header; buttons inside call `cx.stop_propagation()` on mouse down.
- Close the app with the window: `cx.on_window_closed(|cx, _| cx.quit()).detach()`.

## Fonts and assets

- Fonts: `cx.text_system().add_fonts(Vec<Cow<'static, [u8]>>)` with `include_bytes!` TTFs, then
  `.font_family("IBM Plex Sans")`. Files are in `crates/fleet-ui/assets/fonts` (OFL, `OFL.txt`).
- Icons: `svg().path("icons/chevron-left.svg").size(px(11.)).text_color(c)`. The path is resolved by
  the app's `AssetSource` (`fleet_ui::assets::Assets`, a static table). The SVG is used as an alpha mask,
  so stroke colour comes from `text_color`. gpui-component's own `Icon`/`IconName` expects Lucide SVGs at
  `icons/<name>.svg`; our asset source does not serve them yet, so add the needed files (or chain
  `gpui-kit-assets`) before using kit components that draw icons.
- Status glyphs (● ▲ ✕ ◐ ‖ ○) render through system font fallback; Plex lacks some of them.

## Theme

- Fleet tokens: `fleet_ui::theme::Palette` (every HANDOFF token, dark + light), read with
  `cx.palette()` (`ActiveTheme` trait). Sizes are `theme::*` constants; never use literals in screens.
- `theme::init` also pushes fonts, radius and the main colours into `gpui_component::Theme` via
  `Theme::change(mode, None, cx)` then `Theme::update(cx, |t| ...)` (use `update`, not `global_mut`, so
  the kit's tokens and base projection stay in sync). Restyle more kit fields there as screens need them.
- OS appearance: `cx.observe_window_appearance(window, |_, window, cx| theme::follow_window(window, cx))`
  in the root view; `window.appearance()` returns `WindowAppearance::{Light, VibrantLight, Dark, VibrantDark}`.
  `--appearance dark|light` sets `Theme.forced`, which wins over the OS.

## Views, state, async

- `Render::render(&mut self, window, cx: &mut Context<Self>) -> impl IntoElement`.
- Click handlers need an id: `div().id("x").on_click(cx.listener(|this, _ev, window, cx| ...))`.
  Element ids for lists: `ElementId::NamedInteger("cluster".into(), ix as u64)`.
- Backend updates: `cx.spawn(async move |this, cx| while let Ok(u) = rx.recv().await {
  this.update(cx, |view, cx| { ...; cx.notify() })?; })`, keep the `Task` in a field (dropping cancels).
- Closures that borrow `cx` (e.g. `cx.listener`) inside iterator chains: collect to `Vec<AnyElement>`
  first, or the borrow checker rejects the second `cx` use.
- Actions: `gpui::actions!(fleet, [ToggleClustersPanel])`; handle with
  `.on_action(cx.listener(|this, _: &ToggleClustersPanel, window, cx| ...))` on the root div, which must
  `track_focus(&focus_handle)` and be focused (`window.focus(&handle, cx)`), otherwise key bindings
  never reach it.

## Layout and styling gaps

- Tailwind-like API: `.flex().flex_col().flex_1().min_h_0().w(px).h(px).px().gap().rounded()
  .border_b(px(1.)).border_color(c).bg(c).text_size(px).font_weight(FontWeight::SEMIBOLD).truncate()`.
  Scrolling containers need an id: `.id("x").overflow_y_scroll()`.
- No letter-spacing and no text rotation. `kit::VerticalLabel` (the collapsed panels' label) turns
  the Plex SemiBold glyph outlines into one rotated SVG path (`ttf-parser`, `svg().data(..)`), with
  the design's `.07em` tracking, in place of `writing-mode: vertical-rl`.
- No CSS grid; tables should be `uniform_list` rows of fixed-width flex cells.

## Animation

- `element.with_animation(id, Animation::new(dur).with_easing(ease_out_quint()), |el, t| el.w(...))`.
  The animation restarts when the id changes, so the shell bumps a generation counter per toggle and
  uses `ElementId::NamedInteger("clusters-panel", generation)`; generation 0 renders without animation.
  Content inside an animated width uses a fixed width and `overflow_hidden` so it clips instead of
  reflowing.

## Screenshots

`mise run shot -- <fleet|cluster|collapsed> <dark|light>` builds, launches
`fleet --demo --screen <s> --appearance <a>`, finds the window id with `mise-tasks/window-id.swift`
(CGWindowList by pid) and runs `screencapture -l`. Output: `target/shots/<screen>-<appearance>.png`.
`FLEET_FEATURES=loco` builds with Loco. The terminal needs Screen Recording permission.

## Lints

`clippy::pedantic` is on; `match_same_arms` is allowed because AGENTS.md wants one arm per meaning, and
`return_self_not_must_use` because builder methods are everywhere.
