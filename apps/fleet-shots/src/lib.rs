use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow, bail};
use fleet::{Preset, Screen, WINDOW_HEIGHT, WINDOW_WIDTH, bind_keys, root_view};
use fleet_core::{AppState, Backend, Command, Receiver, Sender, Update};
use fleet_demo::DemoBackend;
use fleet_ui::theme::Appearance;
use fleet_ui::{NewWindowRequest, Root, Shell, Store, assets::Assets};
use gpui::{
    AnyView, AnyWindowHandle, App, AppContext, Bounds, Entity, HeadlessAppContext, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput,
    PlatformTextSystem, Point, ScrollDelta, ScrollWheelEvent, Size, TestAppContext, TestDispatcher,
    VisualTestContext, Window, px, size,
};
use image::RgbaImage;

mod fixture;

pub use fixture::StateFixture;

const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(5);
const SEED: u64 = 0;
const SPACE: &str = "space";
const SINGLE: usize = 1;
const DOUBLE: usize = 2;

fn text_system() -> Arc<dyn PlatformTextSystem> {
    Arc::new(gpui_apple::AppleTextSystem::new())
}

fn window_size() -> Size<Pixels> {
    size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT))
}

fn setup(appearance: Appearance, cx: &mut App) -> anyhow::Result<()> {
    fleet_ui::init(Some(appearance), cx)?;
    bind_keys(cx);
    cx.set_reduce_motion(true);
    Ok(())
}

fn demo(state: Option<StateFixture>) -> (Receiver<Update>, Sender<Command>) {
    if let Some(state) = state {
        return fixture::backend(state);
    }
    let (update_tx, update_rx) = async_channel::unbounded();
    let (command_tx, command_rx) = async_channel::unbounded();
    let backend: Box<dyn Backend> = Box::new(DemoBackend);
    backend.start(update_tx, command_rx);
    (update_rx, command_tx)
}

fn view(
    screen: Screen,
    preset: &Preset,
    state: Option<StateFixture>,
    window: &mut Window,
    cx: &mut App,
) -> (AnyView, Option<Entity<Shell>>) {
    let (updates, commands) = demo(state);
    let store = cx.new(|cx| Store::new(updates, commands, cx));
    let view = root_view(screen, preset, store, window, cx);
    let shell = view.clone().downcast::<Shell>().ok();
    (view, shell)
}

fn settle<C: AppContext>(
    cx: &C,
    park: impl Fn(),
    shell: Option<&Entity<Shell>>,
) -> anyhow::Result<()> {
    let start = Instant::now();
    loop {
        park();
        let settled = shell.is_none_or(|shell| cx.read_entity(shell, |shell, _| shell.settled()));
        if settled {
            break;
        }
        if start.elapsed() > SETTLE_TIMEOUT {
            bail!("the demo backend did not settle within {SETTLE_TIMEOUT:?}");
        }
        std::thread::sleep(POLL);
    }
    let quiet = Instant::now();
    while quiet.elapsed() < QUIET {
        park();
        std::thread::sleep(POLL);
    }
    park();
    Ok(())
}

#[derive(Clone, Debug)]
enum Step {
    Keys(String),
    Text(String),
    Click {
        at: Point<Pixels>,
        button: MouseButton,
        count: usize,
        modifiers: Modifiers,
    },
    Hover(Point<Pixels>),
    Scroll(Point<Pixels>, Point<Pixels>),
    Drag {
        from: Point<Pixels>,
        to: Point<Pixels>,
    },
}

fn mouse_down(
    position: Point<Pixels>,
    button: MouseButton,
    count: usize,
    modifiers: Modifiers,
) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent {
        button,
        position,
        modifiers,
        click_count: count,
        first_mouse: false,
    })
}

fn mouse_up(
    position: Point<Pixels>,
    button: MouseButton,
    count: usize,
    modifiers: Modifiers,
) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent {
        button,
        position,
        modifiers,
        click_count: count,
    })
}

fn mouse_move(position: Point<Pixels>, pressed: Option<MouseButton>) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position,
        pressed_button: pressed,
        modifiers: Modifiers::none(),
    })
}

fn parse(keystroke: &str) -> anyhow::Result<Keystroke> {
    Keystroke::parse(keystroke).map_err(|error| anyhow!("keystroke {keystroke:?}: {error}"))
}

fn apply(step: &Step, window: &mut Window, cx: &mut App) -> anyhow::Result<()> {
    match step {
        Step::Keys(keys) => {
            for key in keys.split_whitespace() {
                window.dispatch_keystroke(parse(key)?, cx);
            }
        }
        Step::Text(text) => {
            for char in text.chars() {
                let key = char.to_string();
                let key = if char == ' ' { SPACE.to_owned() } else { key };
                window.dispatch_keystroke(parse(&key)?, cx);
            }
        }
        Step::Click {
            at,
            button,
            count,
            modifiers,
        } => {
            for count in 1..=*count {
                window.dispatch_event(mouse_down(*at, *button, count, *modifiers), cx);
                window.dispatch_event(mouse_up(*at, *button, count, *modifiers), cx);
            }
        }
        Step::Scroll(position, delta) => {
            window.dispatch_event(
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: *position,
                    delta: ScrollDelta::Pixels(*delta),
                    ..ScrollWheelEvent::default()
                }),
                cx,
            );
        }
        Step::Hover(position) => {
            window.dispatch_event(mouse_move(*position, None), cx);
        }
        Step::Drag { from, to } => {
            window.dispatch_event(mouse_move(*from, None), cx);
            let none = Modifiers::none();
            window.dispatch_event(mouse_down(*from, MouseButton::Left, SINGLE, none), cx);
            window.dispatch_event(mouse_move(*to, Some(MouseButton::Left)), cx);
            window.dispatch_event(mouse_up(*to, MouseButton::Left, SINGLE, none), cx);
        }
    }
    Ok(())
}

struct Headless {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    shell: Option<Entity<Shell>>,
}

impl Headless {
    fn open(
        screen: Screen,
        preset: &Preset,
        appearance: Appearance,
        state: Option<StateFixture>,
    ) -> anyhow::Result<Self> {
        let mut cx = HeadlessAppContext::with_platform(
            text_system(),
            Arc::new(Assets),
            gpui_platform::current_headless_renderer,
        );
        cx.update(|cx| setup(appearance, cx))?;
        let mut shell = None;
        let window = cx.open_window(window_size(), |window, cx| {
            let (view, built) = view(screen, preset, state, window, cx);
            shell = built;
            cx.new(|cx| Root::new(view, window, cx))
        })?;
        let mut headless = Self {
            cx,
            window: window.into(),
            shell,
        };
        headless.settle()?;
        Ok(headless)
    }

    fn settle(&mut self) -> anyhow::Result<()> {
        let dispatcher = self.cx.background_executor().clone();
        settle(
            &self.cx,
            || dispatcher.run_until_parked(),
            self.shell.as_ref(),
        )
    }

    fn run(&mut self, step: &Step) -> anyhow::Result<()> {
        self.cx
            .update_window(self.window, |_, window, cx| apply(step, window, cx))??;
        self.settle()
    }

    fn image(&mut self) -> anyhow::Result<RgbaImage> {
        self.cx.capture_screenshot(self.window)
    }
}

pub fn render(
    screen: Screen,
    preset: &Preset,
    appearance: Appearance,
) -> anyhow::Result<RgbaImage> {
    Headless::open(screen, preset, appearance, None)?.image()
}

pub fn render_state(
    screen: Screen,
    preset: &Preset,
    appearance: Appearance,
    state: Option<StateFixture>,
) -> anyhow::Result<RgbaImage> {
    Headless::open(screen, preset, appearance, state)?.image()
}

pub fn save(image: &RgbaImage, path: &Path) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    image
        .save(path)
        .with_context(|| format!("writing {}", path.display()))
}

pub struct Ui {
    cx: VisualTestContext,
    new_windows: Receiver<Vec<Command>>,
    shell: Entity<Shell>,
    screen: Screen,
    preset: Preset,
    appearance: Appearance,
    state: Option<StateFixture>,
    steps: Vec<Step>,
}

impl Ui {
    pub fn open(screen: Screen) -> anyhow::Result<Self> {
        Self::with(screen, Preset::default(), Appearance::Dark)
    }

    pub fn with(screen: Screen, preset: Preset, appearance: Appearance) -> anyhow::Result<Self> {
        Self::build(screen, preset, appearance, None)
    }

    pub fn with_state(screen: Screen, state: StateFixture) -> anyhow::Result<Self> {
        Self::build(screen, Preset::default(), Appearance::Dark, Some(state))
    }

    fn build(
        screen: Screen,
        preset: Preset,
        appearance: Appearance,
        state: Option<StateFixture>,
    ) -> anyhow::Result<Self> {
        let mut app =
            TestAppContext::build_with_text_system(TestDispatcher::new(SEED), None, text_system());
        app.update(|cx| setup(appearance, cx))?;
        let mut shell = None;
        let window = app.open_window(window_size(), |window, cx| {
            let (view, built) = view(screen, &preset, state, window, cx);
            shell = built;
            Root::new(view, window, cx)
        });
        let shell = shell.ok_or_else(|| anyhow!("{screen:?} is not a shell screen"))?;
        let (opened, new_windows) = async_channel::unbounded();
        app.update(|cx| {
            cx.subscribe(&shell, move |_, request: &NewWindowRequest, _| {
                let _ = opened.try_send(request.0.clone());
            })
            .detach();
        });
        let mut ui = Self {
            new_windows,
            cx: VisualTestContext::from_window(window.into(), &app),
            shell,
            screen,
            preset,
            appearance,
            state,
            steps: Vec::new(),
        };
        ui.settle()?;
        Ok(ui)
    }

    pub fn take_new_window(&self) -> Option<Vec<Command>> {
        self.new_windows.try_recv().ok()
    }

    pub fn state(&self) -> AppState {
        self.cx
            .read_entity(&self.shell, |shell, _| shell.state().clone())
    }

    pub fn read<R>(&self, read: impl FnOnce(&Shell, &App) -> R) -> R {
        self.cx.read_entity(&self.shell, read)
    }

    pub fn bounds(&mut self, selector: &'static str) -> Option<Bounds<Pixels>> {
        self.cx.debug_bounds(selector)
    }

    pub fn press(&mut self, keys: &str) -> anyhow::Result<()> {
        self.run(Step::Keys(keys.to_owned()))
    }

    pub fn type_text(&mut self, text: &str) -> anyhow::Result<()> {
        self.run(Step::Text(text.to_owned()))
    }

    pub fn read_app<R>(&self, read: impl FnOnce(&App) -> R) -> R {
        self.cx.read(read)
    }

    fn center(&mut self, selector: &'static str) -> anyhow::Result<Point<Pixels>> {
        self.bounds(selector)
            .map(|bounds| bounds.center())
            .ok_or_else(|| anyhow!("no element with debug selector {selector:?} was painted"))
    }

    pub fn click(&mut self, selector: &'static str) -> anyhow::Result<()> {
        self.click_with(selector, MouseButton::Left, SINGLE, Modifiers::none())
    }

    pub fn double_click(&mut self, selector: &'static str) -> anyhow::Result<()> {
        self.click_with(selector, MouseButton::Left, DOUBLE, Modifiers::none())
    }

    pub fn right_click(&mut self, selector: &'static str) -> anyhow::Result<()> {
        self.click_with(selector, MouseButton::Right, SINGLE, Modifiers::none())
    }

    pub fn middle_click(&mut self, selector: &'static str) -> anyhow::Result<()> {
        self.click_with(selector, MouseButton::Middle, SINGLE, Modifiers::none())
    }

    pub fn cmd_click(&mut self, selector: &'static str) -> anyhow::Result<()> {
        self.click_with(selector, MouseButton::Left, SINGLE, Modifiers::command())
    }

    fn click_with(
        &mut self,
        selector: &'static str,
        button: MouseButton,
        count: usize,
        modifiers: Modifiers,
    ) -> anyhow::Result<()> {
        let at = self.center(selector)?;
        self.run(Step::Click {
            at,
            button,
            count,
            modifiers,
        })
    }

    pub fn hover(&mut self, selector: &'static str) -> anyhow::Result<()> {
        let at = self.center(selector)?;
        self.run(Step::Hover(at))
    }

    pub fn drag(&mut self, selector: &'static str, by: Point<Pixels>) -> anyhow::Result<()> {
        let from = self.center(selector)?;
        self.run(Step::Drag {
            from,
            to: from + by,
        })
    }

    pub fn scroll(&mut self, selector: &'static str, delta: Point<Pixels>) -> anyhow::Result<()> {
        let bounds = self
            .bounds(selector)
            .ok_or_else(|| anyhow!("no element {selector:?}"))?;
        self.run(Step::Scroll(bounds.center(), delta))
    }

    pub fn save_png(&self, path: &Path) -> anyhow::Result<()> {
        let mut headless = Headless::open(self.screen, &self.preset, self.appearance, self.state)?;
        for step in &self.steps {
            headless.run(step)?;
        }
        save(&headless.image()?, path)
    }

    fn run(&mut self, step: Step) -> anyhow::Result<()> {
        self.cx.update(|window, cx| apply(&step, window, cx))?;
        self.steps.push(step);
        self.settle()
    }

    fn settle(&mut self) -> anyhow::Result<()> {
        let dispatcher = self.cx.background_executor.clone();
        settle(
            &self.cx,
            || dispatcher.run_until_parked(),
            Some(&self.shell),
        )
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.cx.quit();
    }
}
