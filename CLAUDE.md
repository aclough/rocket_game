# CLAUDE.md

# Overview

See 'Rocket_Tycoon.md' for what the final game will be like.

Ask clarifying questions when the architecture or parameters are unclear.

For plans always save them as markdown files in the main directory for me to go over offline.  I will make comments as
USER:.  You can ask or them answer questions as CLAUDE until everything is worked out.

# Implementation Approach

When changing fundamental data structures make sure to discuss the changes before implementing them.

When implementing a new game system or feature, implement the full pipeline end-to-end (data generation → logic → signal emission → UI sync) before moving on. After implementation, trace the data flow from creation to display and verify each step.

When asked to use per-item or configurable values, never substitute hardcoded constants. If the user specifies per-flaw probabilities, per-stage thresholds, etc., implement them as data-driven from the start.

Make sure to avoid duplication of calculation and sources of truth for things like time or the money the player has.

# Testing

When modifying physics parameters, game balance values, or constants, always update corresponding test assertions in the same edit. Never leave hardcoded test values that will break due to parameter changes.

# Completion

Always check with the user before commiting a change.

When a feature is good and is approved for check in to git also check TODO.txt to see if it is listed there.  If so
remove it.  Don't update the TODO.txt until just before you go to make a commit.

# Common Pitfalls

When introducing new variables or flags to a struct, search for all places that struct is copied, cloned, or transferred (e.g., design → launcher) and ensure the new field is included. Run `grep` for struct construction sites.

