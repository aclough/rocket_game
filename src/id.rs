//! Id allocation (17_5_G.md step 5 / G1). Every `next_*_id` counter
//! on `Company`, `GameState`, `Manufacturing` and `DesignProject` is an
//! `IdAllocator<T>`: `mint` hands out the next `T` and nothing else
//! reads or bumps the number. The wire form is the bare counter, so every
//! field keeps the name and shape it had.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// An id type that an allocator can mint from its counter.
pub trait RawId {
    fn from_raw(raw: u64) -> Self;
}

/// The next id of one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent, bound = "")]
pub struct IdAllocator<T> {
    next: u64,
    #[serde(skip)]
    _kind: PhantomData<fn() -> T>,
}

impl<T> IdAllocator<T> {
    /// An allocator whose first id carries `next`.
    pub const fn starting_at(next: u64) -> Self {
        IdAllocator { next, _kind: PhantomData }
    }

    /// Mint the next id.
    pub fn mint(&mut self) -> T
    where
        T: RawId,
    {
        let id = T::from_raw(self.next);
        self.next += 1;
        id
    }

    /// The number the next id will carry. For tests and diagnostics;
    /// an id is minted with `mint`, never read from here.
    pub fn next_raw(&self) -> u64 {
        self.next
    }

    /// Move the counter past `raw` if it is not already, so ids minted
    /// elsewhere (a save from before this allocator existed) are never
    /// handed out again.
    pub fn ensure_past(&mut self, raw: u64) {
        if self.next <= raw {
            self.next = raw + 1;
        }
    }
}

/// Zero: the value a save from before a counter existed loads with.
/// Counters that start at one say so with `starting_at(1)`.
impl<T> Default for IdAllocator<T> {
    fn default() -> Self {
        Self::starting_at(0)
    }
}

macro_rules! raw_ids {
    ($($t:ty),* $(,)?) => {
        $(impl RawId for $t {
            fn from_raw(raw: u64) -> Self { Self(raw) }
        })*
    };
}

raw_ids!(
    crate::team::TeamId,
    crate::engine::EngineId,
    crate::engine_project::EngineProjectId,
    crate::flaw::FlawId,
    crate::rocket_project::RocketProjectId,
    crate::third_party::ContractedEngineId,
    crate::reactor_project::ReactorProjectId,
    crate::reactor::ReactorId,
    crate::manufacturing::ManufacturingOrderId,
    crate::manufacturing::InventoryItemId,
    crate::project::ImprovementId,
    crate::contract::ContractId,
    crate::contract::CampaignId,
    crate::flight::FlightId,
    crate::rocket::RocketId,
    crate::game_state::SpacecraftId,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flaw::FlawId;

    #[test]
    fn mints_in_sequence_and_serializes_as_the_bare_counter() {
        let mut ids = IdAllocator::<FlawId>::starting_at(7);
        assert_eq!(ids.mint(), FlawId(7));
        assert_eq!(ids.mint(), FlawId(8));
        assert_eq!(serde_json::to_string(&ids).unwrap(), "9");
        let back: IdAllocator<FlawId> = serde_json::from_str("9").unwrap();
        assert_eq!(back, ids);
    }

    #[test]
    fn ensure_past_only_moves_forward() {
        let mut ids = IdAllocator::<FlawId>::default();
        ids.ensure_past(4);
        assert_eq!(ids.next_raw(), 5);
        ids.ensure_past(2);
        assert_eq!(ids.next_raw(), 5);
    }
}
