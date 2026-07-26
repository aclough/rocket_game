//! Keybinding tables — the single source of truth for what each key
//! does.
//!
//! Four consumers read these tables: the in-pane control hints, the
//! rocket designer's footer, the global help bar, and the `?` help
//! modal. Before M5 the hint strings were hand-written in `draw.rs`
//! and the help modal didn't exist, so the keys were documented in
//! one place and implemented in another.
//!
//! Each binding carries **two** strings, because those consumers want
//! different things. `action` is a full description for the modal,
//! which gives every key its own row. `hint` is the fragment a
//! one-line hint contributes, or `None` for a key the modal alone
//! documents. M5 Task 2 had the hints built from `action`, which
//! assembled a 302-character line for the Engines tab; the pane
//! clipped it mid-word and four of that tab's eight keys were
//! invisible at any normal terminal width. `hint_lines_fit_a_narrow_
//! pane` is the guard.
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
    /// The complete fragment this key contributes to a one-line hint,
    /// e.g. `"[R] Revise"` — or `None` to leave it out of the hint
    /// line entirely.
    ///
    /// This is the whole fragment rather than a short label because
    /// the hint line and the help modal have genuinely different jobs.
    /// The modal gives every key its own row and can afford a
    /// sentence; the hint line has to fit a pane. Owning the fragment
    /// lets a hint merge two keys into one (`[+/-] Team` for the
    /// separate `+` and `-` bindings) and drop the keys nobody needs
    /// on screen, neither of which a bare label could express.
    pub hint: Option<&'static str>,
    /// Imperative one-liner for the help modal: "New engine design",
    /// not "Designs a new engine".
    pub action: &'static str,
    /// Only meaningful when the pane has a selected item. The in-pane
    /// hint line hides these on an empty pane; the help modal always
    /// shows them, since its job is to describe the whole tab.
    pub needs_selection: bool,
}

/// Shorthand for a binding that works whether or not anything is
/// selected.
const fn always(
    keys: &'static str, hint: Option<&'static str>, action: &'static str,
) -> KeyBinding {
    KeyBinding { keys, hint, action, needs_selection: false }
}

/// Shorthand for a binding that acts on the current selection.
const fn on_item(
    keys: &'static str, hint: Option<&'static str>, action: &'static str,
) -> KeyBinding {
    KeyBinding { keys, hint, action, needs_selection: true }
}

/// Keys that work on every tab. These are the bottom help bar, not a
/// pane hint — the bar spans the whole terminal, so it gets the
/// footer's width budget.
///
/// `?` carries no hint of its own because `HELP_HINT` is appended to
/// every line already; listing it here would print it twice.
pub const GLOBAL: &[KeyBinding] = &[
    always("Space", Some("[Space] Pause"), "Pause / resume"),
    always("1 2 3", Some("[1-3] Speed"), "Speed: normal / fast / very fast"),
    always("← →", Some("[←→] Pane"), "Move between the tab list and the pane"),
    always("↑ ↓", Some("[↑↓] Select"), "Move the selection (or scroll)"),
    always("S", Some("[S] Save"), "Save the game"),
    always("?", None, "Show this help"),
    always("F12", Some("[F12] Report"),
        "Write a bug report (state summary + a save to attach)"),
    always("Q", Some("[Q] Quit"), "Quit"),
];

const ENGINES: &[KeyBinding] = &[
    always("N", Some("[N] New"), "New engine design"),
    always("B", Some("[B] 3rd-party"), "Contract a third-party engine"),
    on_item("+", Some("[+/-] Team"),
        "Assign a team (steals from the busiest if none are idle)"),
    on_item("-", None, "Release a team"),
    on_item("R", Some("[R] Revise"),
        "Revise discovered flaws and pending improvements"),
    on_item("O", Some("[O] Order"), "Order a standalone engine build"),
    on_item("A", Some("[A] Auto"), "Toggle auto-revise (on by default)"),
    on_item("E", Some("[E] Hire"), "Hire a new engineering team"),
];

const REACTORS: &[KeyBinding] = &[
    always("N", Some("[N] New"), "New reactor design"),
    on_item("+", Some("[+/-] Team"), "Assign a team"),
    on_item("-", None, "Release a team"),
    on_item("R", Some("[R] Revise"),
        "Revise discovered flaws and pending improvements"),
    on_item("E", Some("[E] Edit"), "Edit the design (In Design only)"),
    on_item("A", Some("[A] Auto"), "Toggle auto-revise (on by default)"),
];

const ROCKETS: &[KeyBinding] = &[
    always("N", Some("[N] New"), "New rocket design (opens the designer)"),
    on_item("+", Some("[+/-] Team"), "Assign a team"),
    on_item("-", None, "Release a team"),
    on_item("R", Some("[R] Revise"), "Revise discovered flaws"),
    on_item("O", Some("[O] Order"), "Order a rocket build"),
    on_item("A", Some("[A] Auto"), "Toggle auto-revise (on by default)"),
    on_item("m", Some("[m] Target"), "Set the auto-build target"),
    // `[M]` rather than `[Shift+M]`: shorter, and sitting next to
    // `[m] Target` it shows the two keys are a case-sensitive pair,
    // which "Shift+M" on its own does not.
    on_item("Shift+M", Some("[M] Modify"),
        "Modify the design (propellant and power only)"),
    // Modal-only. Hiring engineering teams is the same action the
    // Engines pane already advertises, and the rocket-specific keys
    // are worth more of a line this narrow.
    on_item("E", None, "Hire a new engineering team"),
];

const MANUFACTURING: &[KeyBinding] = &[
    always("B", Some("[B] Floor space"), "Buy floor space"),
    always("+", Some("[+/-] Team"), "Assign a manufacturing team"),
    always("-", None, "Release a manufacturing team"),
    always("M", Some("[M] Hire"), "Hire a manufacturing team"),
];

// Contracts and Launches carry their keys in the pane title rather
// than a hint line (see `draw.rs`), so they contribute no hints.
const CONTRACTS: &[KeyBinding] = &[
    on_item("B / A / Enter", None, "Bid on (or accept) the selected contract"),
    always("R", None, "Standing bid rules"),
    always("P", None, "Programs — anchor-customer block bids"),
    always("H", None, "Award history"),
];

const LAUNCHES: &[KeyBinding] = &[
    on_item("L / Enter", None, "Launch the selected rocket"),
    on_item("K", None, "Launch, keeping the carrier as a spacecraft on arrival"),
    on_item("F", None, "Fly a spacecraft to a new destination"),
    on_item("D", None, "Dock one spacecraft onto another"),
    on_item("U", None, "Undock a payload from its carrier"),
    always("P", None, "Delta-v planner"),
];

/// Keys inside the full-screen rocket designer.
///
/// The footer can't carry all fifteen, so the five that are either
/// self-evident from the display (`↑ ↓`) or rarely reached for are
/// modal-only. That selection matches the footer this replaced, which
/// was hand-written in `draw.rs` and had drifted out of the table's
/// reach.
pub const ROCKET_DESIGNER: &[KeyBinding] = &[
    always("↑ ↓", None, "Select a stage"),
    always("Enter", Some("[Enter] Engine"), "Change this stage's engine"),
    always("← →", Some("[←→] Count"), "Fewer / more engines on this stage"),
    always("+ -", Some("[+/-] Prop"), "Less / more propellant"),
    always("V", Some("[V] Nozzle"), "Swap the sea-level and vacuum nozzle"),
    always("A", Some("[A] Add"), "Add a stage on top"),
    always("I", None, "Insert a stage below the selected one"),
    always("B", None, "Add a booster alongside the selected stage"),
    always("W", Some("[W] Power"), "Power sources for this stage"),
    always("X", Some("[X] Rem"), "Remove this stage"),
    always("P", Some("[P] Payload"), "Set the payload mass"),
    always("L", None, "Choose the launch site"),
    always("M", None, "Choose the mission destination"),
    always("D", Some("[D] Done"), "Done — commit the design"),
    always("Esc", Some("[Esc] Cancel"), "Cancel and discard"),
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

/// The escape hatch appended to every hint line, after clipping, so a
/// terminal too narrow for the keys still shows the way to read them.
pub const HELP_HINT: &str = "[?] Keys";

/// The widest hint a pane may produce. A pane is the terminal minus
/// the tab list and two borders, so this is what a 100-column
/// terminal can show with room to spare.
pub const MAX_PANE_HINT: usize = 90;

/// The widest hint the rocket designer's footer may produce. Larger
/// than a pane's because the designer is full-screen — no tab list
/// eating the left edge.
///
/// It is deliberately *not* small enough to guarantee a fit at 100
/// columns: eleven keys don't fit there, and dropping four of them
/// would hide real features (including `[V]`, which M5 Task 1 only
/// just added) from everyone in order to serve the narrowest
/// terminal. Clipping handles the narrow case instead, and
/// `HELP_HINT` survives it.
pub const MAX_FOOTER_HINT: usize = 120;

/// The compact one-line hint shown at the bottom of a pane, e.g.
/// `"[N] New  [B] 3rd-party  [+/-] Team"`.
///
/// Built from each binding's `hint`, not its `action` — the actions
/// are written for the help modal and assemble into a line three
/// times too wide for any terminal. Bindings with no `hint` are
/// modal-only.
///
/// When `has_selection` is false the selection-only keys are dropped,
/// since offering "[+/-] Team" on an empty pane is noise.
///
/// Does **not** include `HELP_HINT`: the caller appends that after
/// clipping to the real width, so it can't be the thing that falls
/// off the edge.
pub fn hint_line(bindings: &[KeyBinding], has_selection: bool) -> String {
    hint_fragments(bindings, has_selection).join("  ")
}

/// The hint fragments that would make up `hint_line`, unjoined, so a
/// caller too narrow to show them all can drop whole fragments rather
/// than cutting one in half. A stranded `[R]…` is a key label with
/// its verb chopped off — it takes up room and tells you nothing.
pub fn hint_fragments(
    bindings: &[KeyBinding], has_selection: bool,
) -> Vec<&'static str> {
    bindings.iter()
        .filter(|b| has_selection || !b.needs_selection)
        .filter_map(|b| b.hint)
        .collect()
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
        assert!(empty.contains("[N] New"));
        assert!(!empty.contains("[R] Revise"),
            "selection-only keys should be hidden, got {empty:?}");

        let full = hint_line(ENGINES, true);
        assert!(full.contains("[R] Revise"));
    }

    /// The bug this file's `hint` field exists to prevent: hint lines
    /// were assembled from `action`, came out at 302 characters for
    /// Engines, and the pane clipped them mid-word — hiding four of
    /// that tab's eight keys on any normal terminal.
    #[test]
    fn hint_lines_fit_a_narrow_pane() {
        let mut cases: Vec<(&str, String, usize)> = Tab::ALL.iter()
            .map(|t| (t.name(), hint_line(for_tab(*t), true), MAX_PANE_HINT))
            .collect();
        // Both span the full terminal rather than a pane.
        cases.push((
            "Rocket designer",
            hint_line(ROCKET_DESIGNER, true),
            MAX_FOOTER_HINT,
        ));
        cases.push(("Global help bar", hint_line(GLOBAL, true), MAX_FOOTER_HINT));

        for (name, line, max) in cases {
            let width = line.chars().count();
            assert!(
                width <= max,
                "{name}: hint line is {width} chars, over the {max} it has \
                 room for — shorten a `hint`, or set one to None and leave \
                 that key to the `?` modal:\n  {line}",
            );
        }
    }

    /// A hint carries its own key label, so a typo'd or missing one
    /// produces a fragment the player can't act on.
    #[test]
    fn every_hint_names_its_key() {
        let all = Tab::ALL.iter().flat_map(|t| for_tab(*t))
            .chain(GLOBAL)
            .chain(ROCKET_DESIGNER);
        for b in all {
            let Some(hint) = b.hint else { continue };
            assert!(
                hint.starts_with('[') && hint.contains(']'),
                "{}: hint {hint:?} should lead with a bracketed key",
                b.keys,
            );
            assert!(
                hint.chars().count() <= 20,
                "{}: hint {hint:?} is long enough to crowd out other keys",
                b.keys,
            );
        }
    }
}
