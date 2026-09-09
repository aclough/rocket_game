//! Campaigns: multi-launch block programmes a market announces, bid
//! on as a block and fulfilled contract by contract.

use super::*;

/// Per-market campaign generation parameters: how often an anchor
/// customer announces a multi-flight program, and what its missions
/// look like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CampaignSpec {
    /// Chance per month (while the market is active and visible) that
    /// a new campaign is announced.
    pub spawn_chance_per_month: f64,
    /// Missions per campaign, drawn uniformly.
    pub mission_count_range: (u32, u32),
    /// Days between mission contract issues, drawn once per campaign.
    pub interval_days_range: (u32, u32),
    /// Block-buy discount off the market rate (0.15 = 15% off),
    /// drawn once per campaign.
    pub discount_range: (f64, f64),
    /// Program name pool ("Meridian Constellation Flight 3").
    pub program_names: Vec<String>,
    /// Days between announcement and block-bid close. Longer than
    /// single-solicitation windows: a program commitment deserves
    /// deliberation time.
    #[serde(default = "default_campaign_bid_window_days")]
    pub bid_window_days: u32,
}

fn default_campaign_bid_window_days() -> u32 {
    30
}

/// A live anchor-customer program: a block of correlated missions
/// (same payload, destination, and per-mission price) competed as one
/// sealed-bid solicitation, then issued on a fixed cadence to the
/// winner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Campaign {
    pub id: CampaignId,
    /// Program name (mission contracts are "{name} Flight {n}").
    pub name: String,
    pub market_id: MarketId,
    pub destination: String,
    pub destination_display: String,
    pub payload_kg: f64,
    /// Per-mission price — the single source of truth for campaign
    /// pricing. Holds the hidden discounted reference while bids are
    /// open, then the winning block bid; mission contracts read it at
    /// issue time.
    pub payment_per_mission: f64,
    pub missions_total: u32,
    pub missions_issued: u32,
    /// Missions the winner let expire — the program-clause strike
    /// count. At `campaign_max_misses` the customer cancels the
    /// remainder.
    #[serde(default)]
    pub missions_missed: u32,
    pub next_issue_date: GameDate,
    pub interval_days: u32,
    #[serde(default = "pre_redesign_campaign_status")]
    pub status: CampaignStatus,
}

/// Lifecycle of a campaign after announcement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CampaignStatus {
    /// Announced; sealed block bids are open until the deadline. The
    /// per-mission ceiling is hidden from the UI (discovery rule),
    /// like a single solicitation's `budget_ceiling`.
    Soliciting {
        bid_deadline: GameDate,
        budget_ceiling_per_mission: f64,
        player_bid: Option<f64>,
    },
    /// Awarded: missions issue on cadence to the winner at the won
    /// `payment_per_mission`.
    Won { by_player: bool, company: String },
}

/// Serde default for saves written before campaigns were competed.
/// No such save can actually contain a campaign (specs shipped as
/// `None` until the redesign), so treat a hypothetical survivor as a
/// player-won program — the closest analog of the old open-offer flow.
fn pre_redesign_campaign_status() -> CampaignStatus {
    CampaignStatus::Won { by_player: true, company: String::new() }
}

/// Roll a campaign announcement for a market. Consumes the spawn roll
/// and, on success, draws the program's parameters and opens the
/// sealed block-bid window. The hidden discounted reference locks in
/// the announcement-time rate multiplier; the per-mission ceiling
/// follows the same rule as single solicitations
/// (reference × `budget_tolerance`).
/// `taken_names` are program names already in use by live campaigns; the
/// draw avoids them so two concurrent programs can't share a name (and
/// therefore can't issue two contracts called "… Flight 1").
pub fn spawn_campaign(
    market: &Market,
    spec: &CampaignSpec,
    rng: &mut StdRng,
    next_campaign_id: &mut u64,
    current_date: GameDate,
    economy_modifier: f64,
    taken_names: &[String],
) -> Option<Campaign> {
    if rng.gen::<f64>() >= spec.spawn_chance_per_month {
        return None;
    }
    if spec.program_names.is_empty() {
        return None;
    }
    let dest = pick_destination(market, rng)?;

    let payload_kg = rng.gen_range(dest.min_payload_kg..=dest.max_payload_kg);
    let payload_kg = ((payload_kg / 100.0).round() * 100.0).max(dest.min_payload_kg);
    let discount = rng.gen_range(spec.discount_range.0..=spec.discount_range.1);
    let rate_mult = market.rate_multiplier(economy_modifier);
    let payment_per_mission =
        round_price(payload_kg * dest.rate_per_kg * rate_mult * (1.0 - discount));
    let missions_total =
        rng.gen_range(spec.mission_count_range.0..=spec.mission_count_range.1);
    let interval_days =
        rng.gen_range(spec.interval_days_range.0..=spec.interval_days_range.1);
    // Prefer a name nobody's using. Falling back to the full pool rather
    // than declining to announce keeps a busy market from going quiet
    // because it ran out of names — with the still-issuing interlock in
    // `advance_month` this shouldn't be reachable anyway, and a duplicate
    // name is a better failure than a missing program.
    let free_names: Vec<&String> = spec.program_names.iter()
        .filter(|n| !taken_names.contains(n))
        .collect();
    let name = if free_names.is_empty() {
        spec.program_names[rng.gen_range(0..spec.program_names.len())].clone()
    } else {
        free_names[rng.gen_range(0..free_names.len())].clone()
    };

    let id = CampaignId(*next_campaign_id);
    *next_campaign_id += 1;

    Some(Campaign {
        id,
        name,
        market_id: market.id,
        destination: dest.location_id.clone(),
        destination_display: dest.display_name.clone(),
        payload_kg,
        payment_per_mission,
        missions_total,
        missions_issued: 0,
        missions_missed: 0,
        next_issue_date: current_date,
        interval_days,
        status: CampaignStatus::Soliciting {
            bid_deadline: current_date.add_days(spec.bid_window_days),
            budget_ceiling_per_mission: payment_per_mission * market.budget_tolerance,
            player_bid: None,
        },
    })
}

/// Issue the campaign's next mission as an ordinary offered contract.
/// The caller advances `missions_issued`/`next_issue_date`.
pub fn campaign_contract(
    campaign: &Campaign,
    deadline_window: (u32, u32),
    rng: &mut StdRng,
    next_contract_id: &mut u64,
    current_date: GameDate,
) -> Contract {
    let deadline_days = rng.gen_range(deadline_window.0..=deadline_window.1);
    let id = ContractId(*next_contract_id);
    *next_contract_id += 1;

    Contract {
        id,
        name: format!(
            "{} Flight {} to {}",
            campaign.name,
            campaign.missions_issued + 1,
            campaign.destination_display,
        ),
        destination: campaign.destination.clone(),
        payload_kg: campaign.payload_kg,
        payment: campaign.payment_per_mission,
        deadline: current_date.add_days(deadline_days),
        status: ContractStatus::Available,
        market_id: campaign.market_id,
        campaign_id: Some(campaign.id),
        // The block was competed once at announcement; individual
        // missions are pre-priced at the won rate, no per-mission
        // bidding.
        bid_deadline: None,
        budget_ceiling: 0.0,
        player_bid: None,
    }
}
