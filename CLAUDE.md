# CLAUDE.md

# Overview

This is a Rust + Godot (GDScript) rocket tycoon game. The Rust code handles game logic, physics, and state; GDScript handles UI and timing. When making changes that cross the Rust/GDScript boundary (signals, state sync), verify both sides are consistent.

See 'Rocket_Tycoon.md' for what the final game will be like.

Ask clarifying questions when the architecture or parameters ars unclear.


# Implementation Approach

When implementing a new game system or feature, implement the full pipeline end-to-end (data generation → logic → signal emission → UI sync) before moving on. After implementation, trace the data flow from creation to display and verify each step.

When asked to use per-item or configurable values, never substitute hardcoded constants. If the user specifies per-flaw probabilities, per-stage thresholds, etc., implement them as data-driven from the start.

Make sure to avoid duplication of calculation and sources of truth for things like time or the money the player has.

# Testing

When modifying physics parameters, game balance values, or constants, always update corresponding test assertions in the same edit. Never leave hardcoded test values that will break due to parameter changes.

After finishing changes to the Godot sections, run
flatpak run org.godotengine.Godot --headless --quit --path /path/to/changes on them.

# Common Pitfalls

When introducing new variables or flags to a struct, search for all places that struct is copied, cloned, or transferred (e.g., design → launcher) and ensure the new field is included. Run `grep` for struct construction sites.

