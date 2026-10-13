use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow, bail};
use fleet::{Preset, Screen, WINDOW_HEIGHT, WINDOW_WIDTH, bind_keys, root_view};
use fleet_core::{AppState, Backend, Command, Receiver, Sender, Update};
use fleet_demo::DemoBackend;
use fleet_ui::theme::Appearance;
use fleet_ui::{Root, Shell, assets::Assets};
use gpui::{
    AnyView, AnyWindowHandle, App, AppContext, Bounds, Entity, HeadlessAppContext, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, PlatformInput,
    PlatformTextSystem, Point, Size, TestAppContext, TestDispatcher, VisualTestContext, Window, px,
    size,
};
use image::RgbaImage;

const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(5);
const SEED: u64 = 0;
const SPACE: &str = "space";

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

fn demo() -> (Receiver<Update>, Sender<Command>) {
    let (update_tx, update_rx) = async_channel::unbounded();
    let (command_tx, command_rx) = async_channel::unbounded();
    let backend: Box<dyn Backend> = Box::new(DemoBackend);
    backend.start(update_tx, command_rx);
    (update_rx, command_tx)
}

fn view(
    screen: Screen,
    preset: &Preset,
    window: &mut Window,
    cx: &mut App,
) -> (AnyView, Option<Entity<Shell>>) {
    let (updates, commands) = demo();
    let view = root_view(screen, preset, updates, commands, window, cx);
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
    Click(Point<Pixels>),
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
        Step::Click(position) => {
            let down = MouseDownEvent {
                button: MouseButton::Left,
                position: *position,
                modifiers: Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            };
            let up = MouseUpEvent {
                button: MouseButton::Left,
                position: *position,
                modifiers: Modifiers::none(),
                click_count: 1,
            };
            window.dispatch_event(PlatformInput::MouseDown(down), cx);
            window.dispatch_event(PlatformInput::MouseUp(up), cx);
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
    fn open(screen: Screen, preset: &Preset, appearance: Appearance) -> anyhow::Result<Self> {
        let mut cx = HeadlessAppContext::with_platform(
            text_system(),
            Arc::new(Assets),
            gpui_platform::current_headless_renderer,
        );
        cx.update(|cx| setup(appearance, cx))?;
        let mut shell = None;
        let window = cx.open_window(window_size(), |window, cx| {
            let (view, built) = view(screen, preset, window, cx);
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
    Headless::open(screen, preset, appearance)?.image()
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
    shell: Entity<Shell>,
    screen: Screen,
    preset: Preset,
    appearance: Appearance,
    steps: Vec<Step>,
}

impl Ui {
    pub fn open(screen: Screen) -> anyhow::Result<Self> {
        Self::with(screen, Preset::default(), Appearance::Dark)
    }

    pub fn with(screen: Screen, preset: Preset, appearance: Appearance) -> anyhow::Result<Self> {
        let mut app =
            TestAppContext::build_with_text_system(TestDispatcher::new(SEED), None, text_system());
        app.update(|cx| setup(appearance, cx))?;
        let mut shell = None;
        let window = app.open_window(window_size(), |window, cx| {
            let (view, built) = view(screen, &preset, window, cx);
            shell = built;
            Root::new(view, window, cx)
        });
        let shell = shell.ok_or_else(|| anyhow!("{screen:?} is not a shell screen"))?;
        let mut ui = Self {
            cx: VisualTestContext::from_window(window.into(), &app),
            shell,
            screen,
            preset,
            appearance,
            steps: Vec::new(),
        };
        ui.settle()?;
        Ok(ui)
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

    pub fn click(&mut self, selector: &'static str) -> anyhow::Result<()> {
        let bounds = self
            .bounds(selector)
            .ok_or_else(|| anyhow!("no element with debug selector {selector:?} was painted"))?;
        self.run(Step::Click(bounds.center()))
    }

    pub fn save_png(&self, path: &Path) -> anyhow::Result<()> {
        let mut headless = Headless::open(self.screen, &self.preset, self.appearance)?;
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
