use std::time::{Duration, Instant};

use gpui::Keystroke;

pub const PAGE_STEP: usize = 10;
pub const TYPE_AHEAD_RESET: Duration = Duration::from_millis(900);
const KEY_SEPARATOR: char = '/';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

impl Nav {
    pub fn parse(keystroke: &Keystroke) -> Option<Self> {
        if keystroke.modifiers.modified() {
            return None;
        }
        match keystroke.key.as_str() {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            "pageup" => Some(Self::PageUp),
            "pagedown" => Some(Self::PageDown),
            _ => None,
        }
    }

    pub fn target(self, current: Option<usize>, len: usize) -> Option<usize> {
        let last = len.checked_sub(1)?;
        let moved = match (self, current) {
            (Self::Home, _) => 0,
            (Self::End, _) => last,
            (Self::Down, None) => 0,
            (Self::Up, None) => last,
            (Self::PageDown, None) => 0,
            (Self::PageUp, None) => 0,
            (Self::Down, Some(at)) => at.saturating_add(1),
            (Self::Up, Some(at)) => at.saturating_sub(1),
            (Self::PageDown, Some(at)) => at.saturating_add(PAGE_STEP),
            (Self::PageUp, Some(at)) => at.saturating_sub(PAGE_STEP),
        };
        Some(moved.min(last))
    }
}

pub fn typed(keystroke: &Keystroke) -> Option<&str> {
    let modifiers = keystroke.modifiers;
    if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
        return None;
    }
    let single = (keystroke.key.chars().count() == 1).then_some(keystroke.key.as_str());
    let text = keystroke.key_char.as_deref().or(single)?;
    let printable = !text.is_empty() && text.chars().all(|c| !c.is_control() && !c.is_whitespace());
    printable.then_some(text)
}

#[derive(Debug, Default)]
pub struct TypeAhead {
    query: String,
    last: Option<Instant>,
}

impl TypeAhead {
    pub fn push(&mut self, text: &str, now: Instant) -> &str {
        let stale = self
            .last
            .is_none_or(|last| now.duration_since(last) >= TYPE_AHEAD_RESET);
        if stale {
            self.query.clear();
        }
        self.query.push_str(&text.to_lowercase());
        self.last = Some(now);
        &self.query
    }
}

pub fn find<'a>(query: &str, names: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    if query.is_empty() {
        return None;
    }
    names.into_iter().position(|name| {
        let name = name.to_lowercase();
        let short = name.rsplit(KEY_SEPARATOR).next().unwrap_or_default();
        name.starts_with(query) || short.starts_with(query)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(source: &str) -> Keystroke {
        Keystroke::parse(source).unwrap_or_else(|error| panic!("{source}: {error}"))
    }

    #[test]
    fn keys_move_within_the_list() {
        let len = 25;
        assert_eq!(Nav::Down.target(None, len), Some(0));
        assert_eq!(Nav::Up.target(None, len), Some(24));
        assert_eq!(Nav::Down.target(Some(24), len), Some(24));
        assert_eq!(Nav::Up.target(Some(0), len), Some(0));
        assert_eq!(Nav::Home.target(Some(7), len), Some(0));
        assert_eq!(Nav::End.target(Some(7), len), Some(24));
        assert_eq!(Nav::PageDown.target(Some(7), len), Some(17));
        assert_eq!(Nav::PageDown.target(Some(20), len), Some(24));
        assert_eq!(Nav::PageUp.target(Some(7), len), Some(0));
        assert_eq!(Nav::Down.target(None, 0), None);
    }

    #[test]
    fn only_bare_navigation_keys_parse() {
        assert_eq!(Nav::parse(&key("pagedown")), Some(Nav::PageDown));
        assert_eq!(Nav::parse(&key("home")), Some(Nav::Home));
        assert_eq!(Nav::parse(&key("cmd-up")), None);
        assert_eq!(Nav::parse(&key("a")), None);
    }

    #[test]
    fn typing_jumps_by_name_and_resets_after_a_pause() {
        let names = ["api-gateway", "payments/checkout-api", "cart", "catalog"];
        let mut ahead = TypeAhead::default();
        let start = Instant::now();
        assert_eq!(find(ahead.push("c", start), names), Some(1));
        assert_eq!(find(ahead.push("a", start), names), Some(2));
        assert_eq!(find(ahead.push("t", start), names), Some(3));
        let later = start + TYPE_AHEAD_RESET;
        assert_eq!(find(ahead.push("A", later), names), Some(0));
        assert_eq!(find(ahead.push("z", later), names), None);
    }

    #[test]
    fn modified_keys_are_not_typing() {
        let mut plain = key("c");
        plain.key_char = Some("c".into());
        assert_eq!(typed(&plain), Some("c"));
        let mut command = key("cmd-c");
        command.key_char = Some("c".into());
        assert_eq!(typed(&command), None);
        assert_eq!(typed(&key("space")), None);
        assert_eq!(typed(&key("k")), Some("k"));
    }
}
