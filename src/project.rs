//! Shared building blocks of the three R&D project types. Grows into
//! the generic `DesignProject<D>` in later steps of `17_1_RD_PIPELINE.md`;
//! for now it holds only what step 1 needs.

use serde::{Deserialize, Serialize};

/// Identity of an improvement within its project. Allocated from the
/// project's own `next_improvement_id`: an improvement never leaves the
/// project that discovered it, so the id only has to be unique there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImprovementId(pub u64);

/// Whether a technology deficiency's stat effect is being put onto a
/// design (at design completion) or taken back off it (when a revision
/// solves the deficiency). The two are exact inverses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Apply,
    Revert,
}
