//! Contracts and the markets that issue them. The contract and award
//! records live here; `market` is the pricing and monthly generation,
//! `campaign` the multi-launch block programmes, `templates` the
//! hand-written markets and `archetype` their seed-perturbed
//! realisation (17_REFACTOR.md E5).

mod archetype;
mod campaign;
mod market;
mod templates;

pub use archetype::*;
pub use campaign::*;
pub use market::*;
pub use templates::*;

use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance_config::MarketsConfig;
use crate::calendar::{GameDate, DAYS_PER_YEAR};
use crate::seed::GameSeed;

/// Unique identifier for a contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractId(pub u64);

/// Unique identifier for a market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct MarketId(pub u64);

/// Status of a contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractStatus {
    Available,
    Accepted,
}

/// Prices on the launch market are quoted to the nearest $10k —
/// payments, bids, and the scripted competitor's prices alike.
pub fn round_price(amount: f64) -> f64 {
    (amount / 10_000.0).round() * 10_000.0
}

/// Unique identifier for an anchor-customer campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct CampaignId(pub u64);

/// A contract to deliver a payload to a destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub name: String,
    pub destination: String,
    pub payload_kg: f64,
    pub payment: f64,
    pub deadline: GameDate,
    pub status: ContractStatus,
    #[serde(default)]
    pub market_id: MarketId,
    /// Set when this contract is a mission of an anchor-customer
    /// campaign (correlated series, block-buy pricing).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub campaign_id: Option<CampaignId>,
    /// When bids close (M3). None = pre-priced accept-at-payment flow
    /// (campaign missions and contracts from pre-M3 saves).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bid_deadline: Option<GameDate>,
    /// The customer's hidden budget ceiling — bids above it lose even
    /// unopposed. Never shown in the UI (discovery rule). Unused when
    /// `bid_deadline` is None.
    #[serde(default)]
    pub budget_ceiling: f64,
    /// The player's sealed bid, revisable until `bid_deadline`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_bid: Option<f64>,
}

impl Contract {
    /// Solicitations are priced by sealed bid; pre-priced contracts
    /// (campaign missions, pre-M3 saves) keep the legacy accept flow.
    pub fn is_solicitation(&self) -> bool {
        self.bid_deadline.is_some()
    }
}

/// One observed award outcome — the player's price-discovery data.
/// Records only what the market made public (or what the player did
/// themselves): winning prices are announced, a rejection reveals
/// nothing but the player's own bid, and hidden ceilings are never
/// stored here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AwardRecord {
    pub date: GameDate,
    pub market_id: MarketId,
    pub contract_name: String,
    pub destination: String,
    pub payload_kg: f64,
    /// Some(n) when this was a campaign block award: the amount is
    /// per mission, n missions in the block. None = single contract.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub missions: Option<u32>,
    pub outcome: AwardOutcome,
}

/// How a solicitation resolved, as observed from the player's seat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AwardOutcome {
    /// The player's bid won at this price.
    PlayerWon { amount: f64 },
    /// A competitor won; the price is public news. `player_bid` is
    /// set when the player bid and lost.
    CompetitorWon { company: String, amount: f64, player_bid: Option<f64> },
    /// The player's bid exceeded the (undisclosed) budget and nobody
    /// else won. Only the player's own bid is knowable.
    PlayerRejected { bid: f64 },
}

/// Get the display name for a location ID.
pub fn destination_display_name(location_id: &str) -> &str {
    crate::location::DELTA_V_MAP.location(location_id)
        .map(|l| l.display_name)
        .unwrap_or(location_id)
}

/// Get the short name for a location ID ("LEO", "GEO", …). Used by
/// tabular views where the full display name ("Low Earth Orbit") is
/// too wide to keep columns aligned.
pub fn destination_short_name(location_id: &str) -> &str {
    crate::location::DELTA_V_MAP.location(location_id)
        .map(|l| l.short_name)
        .unwrap_or(location_id)
}

/// Baseline contract literals for unit tests in other modules.
#[cfg(test)]
pub mod test_support {
    use super::*;

    /// A generic open solicitation; tests override the fields under test
    /// with struct-update syntax.
    pub fn solicitation_fixture() -> Contract {
        Contract {
            id: ContractId(1),
            name: "Test Solicitation".into(),
            destination: "leo".into(),
            payload_kg: 1_000.0,
            payment: 20_000_000.0,
            deadline: GameDate { year: 2001, month: 12, day: 1 },
            status: ContractStatus::Available,
            market_id: MarketId(0),
            campaign_id: None,
            bid_deadline: Some(GameDate { year: 2001, month: 6, day: 1 }),
            budget_ceiling: 24_000_000.0,
            player_bid: None,
        }
    }
}
