//! Keybinding tables — the single source of truth for what each key
//! does.
//!
//! Three consumers read these tables: the in-pane control hints, the
//! rocket designer's footer, and the `?` help modal. Before M5 the
//! hint strings were hand-written in `draw.rs` and the help modal
//! didn't exist, so the keys were documented in one place and
//! implemented in another.
//!
//! **These tables do not dispatch.** The actual behaviour still lives
//! in the `handle_*_key` functions in `ui/mod.rs`, so a key added
//! there and not added here would go undocumented. `keybindings_are_
//! complete` in that module guards the common direction (a documented
//! key that no longer does anything); adding a key without documenting
//! it is caught by review, not by the compiler. If you add a key,
//! add it here in the same edit.

use super::Tab;

/// One documented key.
#[derive(Debug, Clone, Copy)]
pub struct KeyBinding {
    /// How the key is displayed, e.g. `"N"`, `"+/-"`, `"Shift+M"`.
    pub keys: &'static str,
    /// Imperative one-liner: "New engine design", not "Designs a new
    /// engine".
    pub action: &'static str,
    /// Only meaningful when the pane has a selected item. The in-pane
    /// hint line hides these on an empty pane; the help modal always
    /// shows them, since its job is to describe the whole tab.
    pub needs_selection: bool,
}

/// Shorthand for a binding that works whether or not anything is
/// selected.
const fn always(keys: &'static str, action: &'static str) -> KeyBinding {
    KeyBinding { keys, action, needs_selection: false }
}

/// Shorthand for a binding that acts on the current selection.
const fn on_item(keys: &'static str, action: &'static str) -> KeyBinding {
    KeyBinding { keys, action, needs_selection: true }
}

/// Keys that work on every tab.
pub const GLOBAL: &[KeyBinding] = &[
    always("Space", "Pause / resume"),
    always("1 2 3", "Speed: normal / fast / very fast"),
    always("← →", "Move between the tab list and the pane"),
    always("↑ ↓", "Move the selection (or scroll)"),
    always("S", "Save the game"),
    always("?", "Show this help"),
    always("F12", "Write a bug report (state summary + a save to attach)"),
    always("Q", "Quit"),
];

const ENGINES: &[KeyBinding] = &[
    always("N", "New engine design"),
    always("B", "Contract a third-party engine"),
    on_item("+", "Assign a team (steals from the busiest if none are idle)"),
    on_item("-", "Release a team"),
    on_item("R", "Revise discovered flaws and pending improvements"),
    on_item("O", "Order a standalone engine build"),
    on_item("A", "Toggle auto-revise (on by default)"),
    on_item("E", "Hire a new engineering team"),
];

const REACTORS: &[KeyBinding] = &[
    always("N", "New reactor design"),
    on_item("+", "Assign a team"),
    on_item("-", "Release a team"),
    on_item("R", "Revise discovered flaws and pending improvements"),
    on_item("E", "Edit the design (In Design only)"),
    on_item("A", "Toggle auto-revise (on by default)"),
];

const ROCKETS: &[KeyBinding] = &[
    always("N", "New rocket design (opens the designer)"),
    on_item("+", "Assign a team"),
    on_item("-", "Release a team"),
    on_item("R", "Revise discovered flaws"),
    on_item("O", "Order a rocket build"),
    on_item("A", "Toggle auto-revise (on by default)"),
    on_item("m", "Set the auto-build target"),
    on_item("Shift+M", "Modify the design (propellant and power only)"),
    on_item("E", "Hire a new engineering team"),
];

const MANUFACTURING: &[KeyBinding] = &[
    always("B", "Buy floor space"),
    always("+", "Assign a manufacturing team"),
    always("-", "Release a manufacturing team"),
    always("M", "Hire a manufacturing team"),
];

const CONTRACTS: &[KeyBinding] = &[
    on_item("B / A / Enter", "Bid on (or accept) the selected contract"),
    always("R", "Standing bid rules"),
    always("P", "Programs — anchor-customer block bids"),
    always("H", "Award history"),
];

const LAUNCHES: &[KeyBinding] = &[
    on_item("L / Enter", "Launch the selected rocket"),
    on_item("K", "Launch, keeping the carrier as a spacecraft on arrival"),
    on_item("F", "Fly a spacecraft to a new destination"),
    on_item("D", "Dock one spacecraft onto another"),
    on_item("U", "Undock a payload from its carrier"),
    always("P", "Delta-v planner"),
];

/// Keys inside the full-screen rocket designer.
pub const ROCKET_DESIGNER: &[KeyBinding] = &[
    always("↑ ↓", "Select a stage"),
    always("Enter", "Change this stage's engine"),
    always("← →", "Fewer / more engines on this stage"),
    always("+ -", "Less / more propellant"),
    always("V", "Swap the sea-level and vacuum nozzle"),
    always("A", "Add a stage on top"),
    always("I", "Insert a stage below the selected one"),
    always("B", "Add a booster alongside the selected stage"),
    always("W", "Power sources for this stage"),
    always("X", "Remove this stage"),
    always("P", "Set the payload mass"),
    always("L", "Choose the launch site"),
    always("M", "Choose the mission destination"),
    always("D", "Done — commit the design"),
    always("Esc", "Cancel and discard"),
];

/// Tab-specific keys. Overview, Finance, and Events are read-only
/// panes with nothing beyond the global keys.
pub fn for_tab(tab: Tab) -> &'static [KeyBinding] {
    match tab {
        Tab::Overview | Tab::Finance | Tab::Events => &[],
        Tab::Engines => ENGINES,
        Tab::Reactors => REACTORS,
        Tab::Rockets => ROCKETS,
        Tab::Manufacturing => MANUFACTURING,
        Tab::Contracts => CONTRACTS,
        Tab::Launches => LAUNCHES,
    }
}

/// The compact one-line hint shown at the bottom of a pane, e.g.
/// `"[N] New engine design  [B] Contract a third-party engine"`.
/// When `has_selection` is false the selection-only keys are dropped,
/// since offering "[-] Release a team" on an empty pane is noise.
pub fn hint_line(bindings: &[KeyBinding], has_selection: bool) -> String {
    bindings.iter()
        .filter(|b| has_selection || !b.needs_selection)
        .map(|b| format!("[{}] {}", b.keys, b.action))
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tab_has_a_table() {
        // Panics on a Tab variant added without a decision about its
        // keys — the compiler already forces the match arm, this
        // catches a lazy `&[]`.
        for tab in Tab::ALL {
            let bindings = for_tab(*tab);
            let read_only = matches!(
                tab, Tab::Overview | Tab::Finance | Tab::Events,
            );
            assert_eq!(
                bindings.is_empty(), read_only,
                "{} should {} have tab-specific keys",
                tab.name(), if read_only { "not" } else { "" },
            );
        }
    }

    #[test]
    fn descriptions_are_well_formed() {
        let all = Tab::ALL.iter().flat_map(|t| for_tab(*t))
            .chain(GLOBAL)
            .chain(ROCKET_DESIGNER);
        for b in all {
            assert!(!b.keys.is_empty(), "a binding has no key label");
            assert!(!b.action.is_empty(), "{} has no description", b.keys);
            // Descriptions land in a fixed-width modal column; keep
            // them to one readable line.
            assert!(
                b.action.chars().count() <= 62,
                "{}: description too long for the help column ({} chars): {:?}",
                b.keys, b.action.chars().count(), b.action,
            );
        }
    }

    #[test]
    fn hint_line_hides_selection_keys_on_an_empty_pane() {
        let empty = hint_line(ENGINES, false);
        assert!(empty.contains("[N] New engine design"));
        assert!(!empty.contains("Release a team"),
            "selection-only keys should be hidden, got {empty:?}");

        let full = hint_line(ENGINES, true);
        assert!(full.contains("Release a team"));
    }
}
