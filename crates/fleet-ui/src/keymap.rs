use std::fmt::Write as _;

use gpui::KeyBinding;

use crate::actions::{
    ActivateTab, CloseTab, CloseWindow, CompareAcrossClusters, FocusContent, FocusFilter,
    FocusSidebar, FollowLogs, GoBack, GoForward, NewTab, NewWindow, NextTab, OpenSelectedNewWindow,
    OpenSettings, PreviousTab, QuickOpen, Quit, ShowCommands, ShowFluxOwner, ToggleClusterSwitcher,
    ToggleCommandPalette, ToggleDock, ToggleSidebar,
};
use crate::kit::{SendBackTab, SendTab, TERMINAL_CONTEXT};
use crate::screens::command::PALETTE_CONTEXT;
use crate::screens::flux_object::{self, FLUX_CONTEXT};
use crate::screens::workload::{self, KEY_CONTEXT as WORKLOAD_CONTEXT};

type Bind = fn(&'static str, Option<&'static str>) -> KeyBinding;

const OUTSIDE_TEXT: &str = "Shell && !Input && !Terminal";
const SEPARATOR: &str = " / ";
const TYPE_AHEAD: &str = "letters";
const COMMAND_CLICK: &str = "cmd-click";
const MIDDLE_CLICK: &str = "middle click";
const DRAG: &str = "drag";
pub const TAB_KEYS: [&str; 9] = [
    "cmd-1", "cmd-2", "cmd-3", "cmd-4", "cmd-5", "cmd-6", "cmd-7", "cmd-8", "cmd-9",
];
const DOCS_HEADER: &str = "# Keyboard shortcuts\n\nGenerated from the binding table in \
`crates/fleet-ui/src/keymap.rs`; run `mise run gen:keys` after changing it. The same table \
binds the keys and draws the Keyboard shortcuts section of Settings (⌘,).\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    General,
    Tabs,
    View,
    Navigate,
    Lists,
    Resource,
    Palette,
    Workload,
    Terminal,
    Flux,
}

impl Group {
    pub const ALL: [Self; 10] = [
        Self::General,
        Self::Tabs,
        Self::View,
        Self::Navigate,
        Self::Lists,
        Self::Resource,
        Self::Palette,
        Self::Workload,
        Self::Terminal,
        Self::Flux,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Tabs => "Tabs",
            Self::View => "View",
            Self::Navigate => "Navigate",
            Self::Lists => "Lists and tables",
            Self::Resource => "Selected resource",
            Self::Palette => "Command palette",
            Self::Workload => "Workload page",
            Self::Terminal => "Terminal",
            Self::Flux => "Flux object page",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Binding {
    pub keys: &'static [&'static str],
    pub description: &'static str,
    pub group: Group,
    pub context: Option<&'static str>,
    bind: Option<Bind>,
}

impl Binding {
    const fn new(
        keys: &'static [&'static str],
        description: &'static str,
        group: Group,
        bind: Bind,
    ) -> Self {
        Self {
            keys,
            description,
            group,
            context: None,
            bind: Some(bind),
        }
    }

    const fn built_in(
        keys: &'static [&'static str],
        description: &'static str,
        group: Group,
    ) -> Self {
        Self {
            keys,
            description,
            group,
            context: None,
            bind: None,
        }
    }

    const fn within(mut self, context: &'static str) -> Self {
        self.context = Some(context);
        self
    }

    pub fn label(&self) -> String {
        let labels: Vec<String> = self.keys.iter().map(|keys| display(keys)).collect();
        labels.join(SEPARATOR)
    }
}

pub const BINDINGS: &[Binding] = &[
    Binding::new(
        &["cmd-p"],
        "Quick open: resources, clusters and pages",
        Group::General,
        |k, c| KeyBinding::new(k, QuickOpen, c),
    ),
    Binding::new(&["cmd-shift-p"], "Run a command", Group::General, |k, c| {
        KeyBinding::new(k, ShowCommands, c)
    }),
    Binding::new(&["cmd-k"], "Command palette", Group::General, |k, c| {
        KeyBinding::new(k, ToggleCommandPalette, c)
    }),
    Binding::new(
        &["cmd-shift-c"],
        "Switch cluster",
        Group::General,
        |k, c| KeyBinding::new(k, ToggleClusterSwitcher, c),
    ),
    Binding::new(&["cmd-,"], "Settings", Group::General, |k, c| {
        KeyBinding::new(k, OpenSettings, c)
    }),
    Binding::new(&["cmd-n"], "New window", Group::General, |k, c| {
        KeyBinding::new(k, NewWindow, c)
    }),
    Binding::new(
        &["cmd-shift-w"],
        "Close the window",
        Group::General,
        |k, c| KeyBinding::new(k, CloseWindow, c),
    ),
    Binding::new(&["cmd-q"], "Quit Fleet", Group::General, |k, c| {
        KeyBinding::new(k, Quit, c)
    }),
    Binding::new(&["cmd-t"], "New tab on Fleet home", Group::Tabs, |k, c| {
        KeyBinding::new(k, NewTab, c)
    }),
    Binding::new(&["cmd-w"], "Close the tab", Group::Tabs, |k, c| {
        KeyBinding::new(k, CloseTab, c)
    }),
    Binding::new(
        &["ctrl-tab", "cmd-alt-right"],
        "Next tab",
        Group::Tabs,
        |k, c| KeyBinding::new(k, NextTab, c),
    ),
    Binding::new(
        &["ctrl-shift-tab", "cmd-alt-left"],
        "Previous tab",
        Group::Tabs,
        |k, c| KeyBinding::new(k, PreviousTab, c),
    ),
    Binding::new(&TAB_KEYS, "Select tab 1 to 9", Group::Tabs, |k, c| {
        KeyBinding::new(k, ActivateTab(tab_index(k)), c)
    }),
    Binding::built_in(
        &[COMMAND_CLICK, MIDDLE_CLICK],
        "Open a page, row or result in a new tab",
        Group::Tabs,
    ),
    Binding::built_in(
        &[MIDDLE_CLICK],
        "Close the tab under the pointer",
        Group::Tabs,
    ),
    Binding::built_in(&[DRAG], "Reorder tabs", Group::Tabs),
    Binding::new(&["cmd-b"], "Toggle the sidebar", Group::View, |k, c| {
        KeyBinding::new(k, ToggleSidebar, c)
    }),
    Binding::new(&["cmd-j"], "Toggle the bottom dock", Group::View, |k, c| {
        KeyBinding::new(k, ToggleDock, c)
    }),
    Binding::new(
        &["cmd-shift-e"],
        "Focus the sidebar",
        Group::View,
        |k, c| KeyBinding::new(k, FocusSidebar, c),
    ),
    Binding::new(&["cmd-e"], "Focus the page's list", Group::View, |k, c| {
        KeyBinding::new(k, FocusContent, c)
    }),
    Binding::built_in(
        &["tab", "shift-tab"],
        "Next or previous focus area",
        Group::View,
    ),
    Binding::new(&["/"], "Focus the page filter", Group::View, |k, c| {
        KeyBinding::new(k, FocusFilter, c)
    })
    .within(OUTSIDE_TEXT),
    Binding::new(&["cmd-f"], "Focus the page filter", Group::View, |k, c| {
        KeyBinding::new(k, FocusFilter, c)
    }),
    Binding::new(&["cmd-["], "Back", Group::Navigate, |k, c| {
        KeyBinding::new(k, GoBack, c)
    }),
    Binding::new(&["cmd-]"], "Forward", Group::Navigate, |k, c| {
        KeyBinding::new(k, GoForward, c)
    }),
    Binding::built_in(&["up", "down"], "Move the selection", Group::Lists),
    Binding::built_in(&["home", "end"], "First or last row", Group::Lists),
    Binding::built_in(&["pageup", "pagedown"], "Move a page", Group::Lists),
    Binding::built_in(
        &[TYPE_AHEAD],
        "Jump to the row whose name starts with them",
        Group::Lists,
    ),
    Binding::built_in(&["enter"], "Open the selected row", Group::Lists),
    Binding::built_in(&["escape"], "Close the inspector", Group::Lists),
    Binding::built_in(&["cmd-."], "Cluster actions in the switcher", Group::Lists),
    Binding::new(&["cmd-l"], "Follow logs", Group::Resource, |k, c| {
        KeyBinding::new(k, FollowLogs, c)
    }),
    Binding::new(
        &["cmd-shift-d"],
        "Compare across clusters",
        Group::Resource,
        |k, c| KeyBinding::new(k, CompareAcrossClusters, c),
    ),
    Binding::new(
        &["cmd-shift-g"],
        "Show the Flux owner",
        Group::Resource,
        |k, c| KeyBinding::new(k, ShowFluxOwner, c),
    ),
    Binding::built_in(
        &["cmd-enter"],
        "Open the result in a new tab",
        Group::Palette,
    ),
    Binding::new(
        &["alt-enter"],
        "Open the result in a new window",
        Group::Palette,
        |k, c| KeyBinding::new(k, OpenSelectedNewWindow, c),
    )
    .within(PALETTE_CONTEXT),
    Binding::new(&["cmd-."], "Actions", Group::Workload, |k, c| {
        KeyBinding::new(k, workload::ToggleActions, c)
    })
    .within(WORKLOAD_CONTEXT),
    Binding::new(&["cmd-l"], "Logs", Group::Workload, |k, c| {
        KeyBinding::new(k, workload::ShowLogs, c)
    })
    .within(WORKLOAD_CONTEXT),
    Binding::new(&["ctrl-`"], "Terminal", Group::Workload, |k, c| {
        KeyBinding::new(k, workload::ShowTerminal, c)
    })
    .within(WORKLOAD_CONTEXT),
    Binding::new(&["cmd-y"], "YAML", Group::Workload, |k, c| {
        KeyBinding::new(k, workload::ViewYaml, c)
    })
    .within(WORKLOAD_CONTEXT),
    Binding::new(
        &["tab"],
        "Complete in the shell",
        Group::Terminal,
        |k, c| KeyBinding::new(k, SendTab, c),
    )
    .within(TERMINAL_CONTEXT),
    Binding::new(
        &["shift-tab"],
        "Send back-tab to the shell",
        Group::Terminal,
        |k, c| KeyBinding::new(k, SendBackTab, c),
    )
    .within(TERMINAL_CONTEXT),
    Binding::built_in(
        &["cmd-c", "cmd-v"],
        "Copy the transcript, paste into the shell",
        Group::Terminal,
    ),
    Binding::new(&["cmd-r"], "Reconcile", Group::Flux, |k, c| {
        KeyBinding::new(k, flux_object::Reconcile, c)
    })
    .within(FLUX_CONTEXT),
    Binding::new(
        &["cmd-shift-r"],
        "Reconcile with source",
        Group::Flux,
        |k, c| KeyBinding::new(k, flux_object::ReconcileWithSource, c),
    )
    .within(FLUX_CONTEXT),
    Binding::new(&["cmd-d"], "Diff against Git", Group::Flux, |k, c| {
        KeyBinding::new(k, flux_object::DiffAgainstGit, c)
    })
    .within(FLUX_CONTEXT),
    Binding::new(&["cmd-y"], "YAML", Group::Flux, |k, c| {
        KeyBinding::new(k, flux_object::ViewYaml, c)
    })
    .within(FLUX_CONTEXT),
];

fn tab_index(keys: &str) -> usize {
    TAB_KEYS
        .iter()
        .position(|tab| *tab == keys)
        .unwrap_or_default()
}

pub fn key_bindings() -> Vec<KeyBinding> {
    BINDINGS
        .iter()
        .filter_map(|binding| binding.bind.map(|bind| (binding, bind)))
        .flat_map(|(binding, bind)| {
            binding
                .keys
                .iter()
                .map(move |keys| bind(keys, binding.context))
        })
        .collect()
}

pub fn grouped() -> Vec<(Group, Vec<&'static Binding>)> {
    Group::ALL
        .into_iter()
        .map(|group| {
            let rows = BINDINGS.iter().filter(|b| b.group == group).collect();
            (group, rows)
        })
        .collect()
}

pub fn display(keys: &str) -> String {
    let mut parts: Vec<&str> = keys.split('-').collect();
    let key = match parts.pop() {
        Some("") => "-",
        Some(key) => key,
        None => "",
    };
    let has = |modifier: &str| parts.contains(&modifier);
    let mut label = String::new();
    for (modifier, glyph) in [("ctrl", "⌃"), ("alt", "⌥"), ("cmd", "⌘"), ("shift", "⇧")] {
        if has(modifier) {
            label.push_str(glyph);
        }
    }
    label.push_str(&key_name(key));
    label
}

fn key_name(key: &str) -> String {
    let named = match key {
        "enter" => "↵",
        "escape" => "Esc",
        "up" => "↑",
        "down" => "↓",
        "left" => "←",
        "right" => "→",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PgUp",
        "pagedown" => "PgDn",
        "tab" => "Tab",
        "space" => "Space",
        "click" => "-click",
        other if other.chars().count() == 1 => return other.to_uppercase(),
        other => other,
    };
    named.to_owned()
}

pub fn markdown() -> String {
    let mut doc = String::from(DOCS_HEADER);
    for (group, rows) in grouped() {
        let _ = write!(
            doc,
            "\n## {}\n\n| Keys | Action |\n|---|---|\n",
            group.label()
        );
        for binding in rows {
            let _ = writeln!(doc, "| {} | {} |", binding.label(), binding.description);
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use fleet_core::palette::{COMPARE_KEY, LOGS_KEY, OWNER_KEY};

    use super::*;

    const DOCS: &str = "../../docs/keyboard.md";
    const UPDATE_VAR: &str = "FLEET_UPDATE_DOCS";

    #[test]
    fn the_filter_key_stays_out_of_terminals_and_text_fields() {
        assert!(OUTSIDE_TEXT.contains(&format!("!{TERMINAL_CONTEXT}")));
        assert!(OUTSIDE_TEXT.starts_with(crate::shell::KEY_CONTEXT));
    }

    #[test]
    fn labels_use_mac_glyphs() {
        assert_eq!(display("cmd-shift-p"), "⌘⇧P");
        assert_eq!(display("ctrl-shift-tab"), "⌃⇧Tab");
        assert_eq!(display("cmd-["), "⌘[");
        assert_eq!(display("cmd-,"), "⌘,");
        assert_eq!(display("/"), "/");
        assert_eq!(display("alt-enter"), "⌥↵");
        assert_eq!(display("pagedown"), "PgDn");
        assert_eq!(display(TYPE_AHEAD), TYPE_AHEAD);
        assert_eq!(display(COMMAND_CLICK), "⌘-click");
        assert_eq!(display("cmd-alt-right"), "⌥⌘→");
        assert_eq!(display("ctrl-`"), "⌃`");
        assert_eq!(tab_index("cmd-1"), 0);
        assert_eq!(tab_index("cmd-9"), 8);
    }

    #[test]
    fn every_binding_parses_and_no_keys_collide_in_one_context() {
        let bindings = key_bindings();
        let bound: usize = BINDINGS
            .iter()
            .filter(|b| b.bind.is_some())
            .map(|b| b.keys.len())
            .sum();
        assert_eq!(bindings.len(), bound);
        let mut seen = HashSet::new();
        for binding in BINDINGS.iter().filter(|b| b.bind.is_some()) {
            for keys in binding.keys {
                assert!(seen.insert((*keys, binding.context)), "{keys} bound twice");
            }
        }
    }

    #[test]
    fn the_vs_code_bindings_are_in_the_table() {
        let bound: HashSet<&str> = BINDINGS
            .iter()
            .filter(|b| b.bind.is_some() && b.context.is_none())
            .flat_map(|b| b.keys.iter().copied())
            .collect();
        for keys in [
            "cmd-p",
            "cmd-shift-p",
            "cmd-b",
            "cmd-j",
            "cmd-w",
            "cmd-,",
            "ctrl-tab",
            "cmd-t",
            "cmd-1",
            "cmd-[",
            "cmd-]",
            "cmd-f",
            "cmd-k",
        ] {
            assert!(bound.contains(keys), "{keys} is not bound");
        }
    }

    #[test]
    fn palette_hints_match_the_table() {
        let label = |description: &str| {
            BINDINGS
                .iter()
                .find(|b| b.description == description)
                .map(Binding::label)
        };
        assert_eq!(label("Follow logs").as_deref(), Some(LOGS_KEY));
        assert_eq!(
            label("Compare across clusters").as_deref(),
            Some(COMPARE_KEY)
        );
        assert_eq!(label("Show the Flux owner").as_deref(), Some(OWNER_KEY));
    }

    #[test]
    fn docs_are_generated_from_the_table() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOCS);
        let generated = markdown();
        if std::env::var_os(UPDATE_VAR).is_some() {
            assert!(std::fs::write(&path, &generated).is_ok());
        }
        let written = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            written == generated,
            "docs/keyboard.md is stale; run `mise run gen:keys`"
        );
    }
}
