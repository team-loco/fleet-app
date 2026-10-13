use gpui::{
    Animation, AnimationExt, AnyElement, Context, ElementId, IntoElement, ParentElement, Pixels,
    SharedString, Styled, Task, div, ease_out_quint,
};

use crate::theme::{INSPECTOR_IN, INSPECTOR_OUT, INSPECTOR_SLIDE, INSPECTOR_SWAP};

const ENTER: &str = "enter";
const LEAVE: &str = "leave";
const SWAP: &str = "swap";

pub struct Presence<T> {
    shown: Option<T>,
    leaving: Option<T>,
    exit: Option<Task<()>>,
}

impl<T> Default for Presence<T> {
    fn default() -> Self {
        Self {
            shown: None,
            leaving: None,
            exit: None,
        }
    }
}

impl<T: Clone + 'static> Presence<T> {
    pub fn track<V: 'static>(
        &mut self,
        current: Option<&T>,
        field: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) {
        let previous = std::mem::replace(&mut self.shown, current.cloned());
        if current.is_some() {
            self.leaving = None;
            self.exit = None;
            return;
        }
        let Some(previous) = previous else {
            return;
        };
        if cx.reduce_motion() {
            return;
        }
        self.leaving = Some(previous);
        self.exit = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(INSPECTOR_OUT).await;
            this.update(cx, |view, cx| {
                field(view).leaving = None;
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn leaving(&self) -> Option<&T> {
        self.leaving.as_ref()
    }
}

fn key(id: &str, phase: &str, selection: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{id}-{phase}-{selection}")))
}

pub fn swap_in(element: impl IntoElement, id: &str, selection: &str) -> AnyElement {
    let animation = Animation::new(INSPECTOR_SWAP).with_easing(ease_out_quint());
    div()
        .relative()
        .size_full()
        .child(element)
        .with_animation(key(id, SWAP, selection), animation, |content, delta| {
            content.opacity(delta).left(INSPECTOR_SLIDE * (1. - delta))
        })
        .into_any_element()
}

pub fn slide_in(element: impl IntoElement, id: &str, selection: &str, width: Pixels) -> AnyElement {
    let animation = Animation::new(INSPECTOR_IN).with_easing(ease_out_quint());
    let content = div()
        .flex_none()
        .w(width)
        .h_full()
        .child(swap_in(element, id, selection));
    div()
        .flex_none()
        .h_full()
        .overflow_hidden()
        .w(width)
        .child(content)
        .with_animation(key(id, ENTER, ""), animation, move |panel, delta| {
            panel.w(width * delta).opacity(delta)
        })
        .into_any_element()
}

pub fn slide_out(element: impl IntoElement, id: &str, width: Pixels) -> AnyElement {
    let animation = Animation::new(INSPECTOR_OUT).with_easing(ease_out_quint());
    let content = div().flex_none().w(width).h_full().child(element);
    div()
        .flex_none()
        .h_full()
        .overflow_hidden()
        .w(width)
        .child(content)
        .with_animation(key(id, LEAVE, ""), animation, move |panel, delta| {
            let shown = 1. - delta;
            panel.w(width * shown).opacity(shown)
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext};

    use super::*;

    #[derive(Default)]
    struct View {
        presence: Presence<&'static str>,
    }

    fn presence(view: &mut View) -> &mut Presence<&'static str> {
        &mut view.presence
    }

    fn track(view: &gpui::Entity<View>, current: Option<&'static str>, cx: &mut TestAppContext) {
        view.update(cx, |view, cx| {
            view.presence.track(current.as_ref(), presence, cx);
        });
    }

    fn leaving(view: &gpui::Entity<View>, cx: &mut TestAppContext) -> Option<&'static str> {
        view.read_with(cx, |view, _| view.presence.leaving().copied())
    }

    #[gpui::test]
    fn a_closed_inspector_lingers_until_its_exit_finishes(cx: &mut TestAppContext) {
        let view = cx.new(|_| View::default());
        track(&view, Some("a"), cx);
        track(&view, Some("b"), cx);
        assert_eq!(leaving(&view, cx), None);
        track(&view, None, cx);
        assert_eq!(leaving(&view, cx), Some("b"));
        track(&view, None, cx);
        assert_eq!(leaving(&view, cx), Some("b"));
        cx.executor().advance_clock(INSPECTOR_OUT);
        cx.run_until_parked();
        assert_eq!(leaving(&view, cx), None);
    }

    #[gpui::test]
    fn selecting_again_cancels_the_exit(cx: &mut TestAppContext) {
        let view = cx.new(|_| View::default());
        track(&view, Some("a"), cx);
        track(&view, None, cx);
        track(&view, Some("c"), cx);
        assert_eq!(leaving(&view, cx), None);
        cx.executor().advance_clock(INSPECTOR_OUT);
        cx.run_until_parked();
        track(&view, Some("c"), cx);
        assert_eq!(leaving(&view, cx), None);
    }

    #[gpui::test]
    fn reduced_motion_removes_the_inspector_at_once(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_reduce_motion(true));
        let view = cx.new(|_| View::default());
        track(&view, Some("a"), cx);
        track(&view, None, cx);
        assert_eq!(leaving(&view, cx), None);
    }
}
