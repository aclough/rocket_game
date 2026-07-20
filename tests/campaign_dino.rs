//! Campaign redesign Task 2: DinoSoar competing for anchor-customer
//! campaigns.
//!
//! `tests/campaigns.rs` locks the campaign pipeline with the player as
//! sole bidder; `tests/competitor_dino.rs` locks DinoSoar's single-
//! solicitation bidding. This file is the intersection: DinoSoar's
//! sealed block bid (`Competitor::compute_block_bid`) competing
//! against the player's sealed campaign bid in
//! `GameState::resolve_campaign_bids` — award competition (dino
//! outbids an expensive player, an aggressive player undercuts dino,
//! an exact tie breaks to the player), dino declining a
//! hopelessly-underfunded small-payload program, the winner's
//! missions issuing on cadence and launching abstractly through
//! `process_competitor_launches`, save/load round-trip of a
//! competitor-won campaign, and resolution determinism given a fixed
//! seed.
//!
//! Pattern: campaigns are injected directly into `gs.active_campaigns`
//! as hand-built `Campaign` literals (mirroring how
//! `tests/competitor_dino.rs` injects `Contract` solicitations
//! directly into `available_contracts`), so bid deadlines, ceilings,
//! and payloads are fully under test control. Bid windows are kept to
//! 2 days so DinoSoar's free stock/marginal cost can't drift between
//! fixture setup and resolution.

use std::collections::HashSet;

use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::contract::{
    Campaign, CampaignId, CampaignStatus, ContractId, ContractStatus,
    MARKET_GEO_COMSATS, MARKET_RIDESHARE,
};
use rocket_tycoon::event::GameEvent;
use rocket_tycoon::game_state::{GameSpeed, GameState};

/// A fresh game under default balance (DinoSoar enabled) at `seed`.
fn fresh_game(seed: u64) -> GameState {
    GameState::new("Test".into(), 200_000_000.0, seed)
}

/// Build a contested GEO Comsats campaign: gto, 5,000 kg (comfortably
/// under DinoSoar's 13,500 kg gto cap), a $200M hidden per-mission
/// reference, a $240M ceiling, 3 missions on a 30-day cadence, and a
/// sealed bid window closing at `bid_deadline`.
fn contested_campaign(id: u64, name: &str, bid_deadline: GameDate, next_issue_date: GameDate) -> Campaign {
    Campaign {
        id: CampaignId(id),
        name: name.into(),
        market_id: MARKET_GEO_COMSATS,
        destination: "gto".into(),
        destination_display: "GTO".into(),
        payload_kg: 5_000.0,
        payment_per_mission: 200_000_000.0,
        missions_total: 3,
        missions_issued: 0,
        missions_missed: 0,
        next_issue_date,
        interval_days: 30,
        status: CampaignStatus::Soliciting {
            bid_deadline,
            budget_ceiling_per_mission: 240_000_000.0,
            player_bid: None,
        },
    }
}

fn temp_save_path(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("rocket_tycoon_test");
    std::fs::create_dir_all(&dir).unwrap();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("campaign_dino_{tag}_{}_{n}.json", std::process::id()))
}

// ---------------------------------------------------------------
// 1. Dino outbids an expensive player.
// ---------------------------------------------------------------

/// Locks the losing side of block-bid competition: when the player's
/// sealed campaign bid is well above DinoSoar's scripted block bid,
/// DinoSoar wins the whole program, the award event carries the
/// losing player bid, the campaign becomes `Won { by_player: false }`
/// at DinoSoar's price, Flight 1 issues into DinoSoar's active
/// contracts with a scheduled launch, and the player gets nothing.
#[test]
fn dino_outbids_expensive_player() {
    let seed = 7101;
    let mut gs = fresh_game(seed);
    assert_eq!(gs.competitors.len(), 1, "seed {seed}: expected exactly one competitor");

    let bid_deadline = gs.date.add_days(2);
    let cid = CampaignId(7001);
    let program = "Orbital Relay Program";
    gs.active_campaigns.push(contested_campaign(7001, program, bid_deadline, gs.date));

    // Size the player's bid off dino's bid at fixture setup (fresh
    // game: free stock and marginal cost are at their starting
    // values).
    let snapshot = gs.active_campaigns.last().unwrap().clone();
    let setup_bid = gs.competitors[0]
        .compute_block_bid(&snapshot, &gs.balance, &gs.seed)
        .expect("seed 7101: DinoSoar should be willing to block-bid a fresh gto campaign");
    let player_bid = setup_bid * 1.5;
    assert!(player_bid <= 240_000_000.0, "fixture invariant: player bid must still fit the ceiling");
    assert!(gs.place_campaign_bid(cid, player_bid).is_some(), "seed {seed}: player bid should be accepted");

    // Advance up to (not past) the deadline, then recompute dino's
    // expected bid right before the tick that resolves it — that's
    // the free stock/cost basis actually scored.
    while gs.date < bid_deadline {
        gs.advance_day();
    }
    let campaign_now = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap().clone();
    let expected_bid = gs.competitors[0]
        .compute_block_bid(&campaign_now, &gs.balance, &gs.seed)
        .expect("seed 7101: DinoSoar should still be willing to bid right before resolution");

    let events = gs.advance_day();

    let award = events.iter().find_map(|e| match e {
        GameEvent::CampaignAwardedToCompetitor { program: p, company, amount, missions, player_bid: pb }
            if p == program => Some((company.clone(), *amount, *missions, *pb)),
        _ => None,
    });
    let (company, amount, missions, pb) = award.unwrap_or_else(|| {
        panic!("seed {seed}: expected CampaignAwardedToCompetitor for {program}, got {events:?}")
    });
    assert_eq!(company, "DinoSoar", "seed {seed}: winner should be DinoSoar");
    assert_eq!(amount, expected_bid, "seed {seed}: awarded amount should equal dino's pre-resolution block bid");
    assert_eq!(missions, 3, "seed {seed}: event should carry the program's full mission count");
    assert_eq!(pb, Some(player_bid), "seed {seed}: the losing player bid should ride along in the event");

    let campaign = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
    assert_eq!(campaign.payment_per_mission, expected_bid, "seed {seed}: won price becomes the block price");
    match &campaign.status {
        CampaignStatus::Won { by_player, company } => {
            assert!(!by_player, "seed {seed}: by_player should be false");
            assert_eq!(company, "DinoSoar");
        }
        other => panic!("seed {seed}: expected Won by DinoSoar, got {other:?}"),
    }

    let flight1_name = format!("{program} Flight 1 to GTO");
    let dino_contract = gs.competitors[0].company.active_contracts.iter().find(|c| c.name == flight1_name)
        .unwrap_or_else(|| panic!("seed {seed}: Flight 1 should be pre-issued into DinoSoar's active_contracts"));
    assert!(
        gs.competitors[0].scheduled_launches.iter().any(|sl| sl.contract_id == dino_contract.id),
        "seed {seed}: DinoSoar should have a scheduled launch for Flight 1",
    );

    assert!(
        !gs.player_company.active_contracts.iter().any(|c| c.campaign_id == Some(cid)),
        "seed {seed}: player should hold no contract from a campaign it lost",
    );
    assert!(
        !gs.available_contracts.iter().any(|c| c.campaign_id == Some(cid)),
        "seed {seed}: a resolved campaign's missions never pass through the open market",
    );

    // The loss lands in the price-discovery history: winner's public
    // per-mission price, the player's own losing bid, block size.
    let record = gs.award_history.iter().rev()
        .find(|r| r.contract_name == program)
        .expect("block loss should be recorded");
    assert_eq!(record.missions, Some(3));
    match &record.outcome {
        rocket_tycoon::contract::AwardOutcome::CompetitorWon { company, amount, player_bid: pb } => {
            assert_eq!(company, "DinoSoar");
            assert_eq!(*amount, expected_bid);
            assert_eq!(*pb, Some(player_bid));
        }
        other => panic!("seed {seed}: expected CompetitorWon record, got {other:?}"),
    }
}

// ---------------------------------------------------------------
// 2. Player undercuts Dino and wins.
// ---------------------------------------------------------------

/// Locks the winning side: an aggressive player bid below DinoSoar's
/// scripted block bid wins the whole program, pauses the game
/// (a scheduling decision point), and issues Flight 1 pre-accepted at
/// the player's price.
#[test]
fn player_undercut_wins() {
    let seed = 7102;
    let mut gs = fresh_game(seed);

    let bid_deadline = gs.date.add_days(2);
    let cid = CampaignId(7002);
    let program = "Aurora Constellation";
    gs.active_campaigns.push(contested_campaign(7002, program, bid_deadline, gs.date));

    let snapshot = gs.active_campaigns.last().unwrap().clone();
    let dino_expected = gs.competitors[0]
        .compute_block_bid(&snapshot, &gs.balance, &gs.seed)
        .expect("seed 7102: DinoSoar should be willing to block-bid a fresh gto campaign");

    let player_bid = dino_expected * 0.5;
    assert!(gs.place_campaign_bid(cid, player_bid).is_some(), "seed {seed}: player bid should be accepted");

    while gs.date < bid_deadline {
        gs.advance_day();
    }
    let events = gs.advance_day();

    let award = events.iter().find_map(|e| match e {
        GameEvent::CampaignAwarded { program: p, amount, missions } if p == program =>
            Some((*amount, *missions)),
        _ => None,
    });
    let (amount, missions) = award.unwrap_or_else(|| {
        panic!("seed {seed}: expected player CampaignAwarded for {program}, got {events:?}")
    });
    assert_eq!(amount, player_bid, "seed {seed}: awarded amount should be the player's bid");
    assert_eq!(missions, 3, "seed {seed}: event should carry the program's full mission count");

    let campaign = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
    match &campaign.status {
        CampaignStatus::Won { by_player, .. } => assert!(*by_player, "seed {seed}: by_player should be true"),
        other => panic!("seed {seed}: expected player Won, got {other:?}"),
    }
    assert_eq!(gs.speed, GameSpeed::Paused, "seed {seed}: winning a program pauses the game");

    let flight1_name = format!("{program} Flight 1 to GTO");
    let c = gs.player_company.active_contracts.iter().find(|c| c.name == flight1_name)
        .unwrap_or_else(|| panic!("seed {seed}: Flight 1 should be pre-accepted in the player's active_contracts"));
    assert_eq!(c.payment, player_bid, "seed {seed}: missions must pay the won block price");
    assert!(matches!(c.status, ContractStatus::Accepted), "seed {seed}: campaign missions arrive pre-accepted");

    assert!(
        !gs.competitors[0].company.active_contracts.iter().any(|c| c.campaign_id == Some(cid)),
        "seed {seed}: DinoSoar should not have the campaign it lost",
    );
}

// ---------------------------------------------------------------
// 3. Exact tie goes to the player.
// ---------------------------------------------------------------

/// Player and DinoSoar start with identical reputation, so an exact
/// price tie scores identically; the resolution order (player
/// considered first, replacement requires strict `>`) must break the
/// tie to the player.
#[test]
fn exact_tie_goes_to_player() {
    let seed = 7103;
    let mut gs = fresh_game(seed);

    let bid_deadline = gs.date.add_days(2);
    let cid = CampaignId(7003);
    let program = "Meridian Fleet";
    gs.active_campaigns.push(contested_campaign(7003, program, bid_deadline, gs.date));

    let snapshot = gs.active_campaigns.last().unwrap().clone();
    let dino_expected = gs.competitors[0]
        .compute_block_bid(&snapshot, &gs.balance, &gs.seed)
        .expect("seed 7103: DinoSoar should be willing to block-bid a fresh gto campaign");
    assert!(gs.place_campaign_bid(cid, dino_expected).is_some(), "seed {seed}: player bid should be accepted");

    while gs.date < bid_deadline {
        gs.advance_day();
    }
    // Confirm dino's bid hasn't drifted within the 2-day window, so
    // the tie really is exact at resolution.
    let campaign_now = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap().clone();
    let dino_now = gs.competitors[0]
        .compute_block_bid(&campaign_now, &gs.balance, &gs.seed)
        .expect("seed 7103: DinoSoar should still be willing to bid right before resolution");
    assert_eq!(dino_now, dino_expected, "seed {seed}: dino's bid shouldn't drift within a 2-day window");

    let events = gs.advance_day();
    let award = events.iter().find_map(|e| match e {
        GameEvent::CampaignAwarded { program: p, amount, .. } if p == program => Some(*amount),
        _ => None,
    });
    let amount = award.unwrap_or_else(|| {
        panic!("seed {seed}: expected the tied player bid to win {program}, got {events:?}")
    });
    assert_eq!(amount, dino_expected, "seed {seed}: awarded amount should be the tied price");

    let campaign = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
    match &campaign.status {
        CampaignStatus::Won { by_player, .. } => assert!(*by_player, "seed {seed}: tie should break to the player"),
        other => panic!("seed {seed}: expected player Won, got {other:?}"),
    }
}

// ---------------------------------------------------------------
// 4. Dino ignores a hopelessly small-payload block.
// ---------------------------------------------------------------

/// DinoSoar can technically lift 300 kg to LEO (well under its
/// 26,000 kg leo cap), so `compute_block_bid` still returns a price —
/// but that price is floored at `bid_floor` ($60M) and hopelessly over
/// a rideshare-scale ceiling, so the bid never scores at resolution
/// and, with no player bid either, the whole program lapses quietly.
#[test]
fn dino_ignores_small_payload_blocks() {
    let seed = 7104;
    let mut gs = fresh_game(seed);

    let bid_deadline = gs.date.add_days(2);
    let cid = CampaignId(7004);
    let ceiling = 4_800_000.0;
    let campaign = Campaign {
        id: cid,
        name: "Cubesat Rideshare Co-op".into(),
        market_id: MARKET_RIDESHARE,
        destination: "leo".into(),
        destination_display: "LEO".into(),
        payload_kg: 300.0,
        payment_per_mission: 4_000_000.0,
        missions_total: 3,
        missions_issued: 0,
        missions_missed: 0,
        next_issue_date: gs.date,
        interval_days: 30,
        status: CampaignStatus::Soliciting {
            bid_deadline,
            budget_ceiling_per_mission: ceiling,
            player_bid: None,
        },
    };
    gs.active_campaigns.push(campaign.clone());

    let bid = gs.competitors[0]
        .compute_block_bid(&campaign, &gs.balance, &gs.seed)
        .expect("seed 7104: DinoSoar can lift 300kg to LEO, so it still prices the block");
    assert!(
        bid >= gs.balance.competitor.bid_floor,
        "seed {seed}: bid ${bid} should sit at or above the bid floor",
    );
    assert!(
        bid > ceiling,
        "seed {seed}: floored bid ${bid} should be hopelessly over the ${ceiling} ceiling",
    );

    while gs.date < bid_deadline {
        gs.advance_day();
    }
    let events = gs.advance_day();

    assert!(
        gs.active_campaigns.iter().all(|c| c.id != cid),
        "seed {seed}: an over-ceiling, unbid campaign should lapse",
    );
    assert!(
        !events.iter().any(|e| matches!(e, GameEvent::CampaignBidRejected { .. })),
        "seed {seed}: lapsing without a player bid should not announce a rejection",
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            GameEvent::CampaignAwarded { .. } | GameEvent::CampaignAwardedToCompetitor { .. },
        )),
        "seed {seed}: a campaign with no bid under ceiling should never be awarded",
    );
}

// ---------------------------------------------------------------
// 5. Dino's block missions issue and launch on cadence.
// ---------------------------------------------------------------

/// After DinoSoar wins a program unopposed, its missions must issue
/// on the program's cadence, each schedule a launch, and those
/// launches must actually fire through `process_competitor_launches`
/// (success or failure both count — this locks the abstract launch
/// pipeline, not the reliability roll). Once all missions have issued
/// and flown, the campaign retires and no scheduled launch for one of
/// its missions should remain outstanding.
#[test]
fn dino_block_missions_launch_on_cadence() {
    let seed = 7105;
    let mut gs = fresh_game(seed);

    let bid_deadline = gs.date.add_days(2);
    let cid = CampaignId(7005);
    let program = "Sentinel Constellation";
    gs.active_campaigns.push(contested_campaign(7005, program, bid_deadline, gs.date));
    // No player bid: DinoSoar wins unopposed.

    while gs.date < bid_deadline {
        gs.advance_day();
    }
    let award_events = gs.advance_day();
    assert!(
        award_events.iter().any(|e| matches!(
            e, GameEvent::CampaignAwardedToCompetitor { program: p, .. } if p == program,
        )),
        "seed {seed}: expected DinoSoar to win {program} unopposed, got {award_events:?}",
    );

    let flight_names: Vec<String> = (1..=3).map(|n| format!("{program} Flight {n} to GTO")).collect();

    let mut mission_ids: HashSet<ContractId> = HashSet::new();
    let mut launches_seen: HashSet<String> = HashSet::new();
    for _ in 0..150 {
        for c in &gs.competitors[0].company.active_contracts {
            if c.campaign_id == Some(cid) {
                mission_ids.insert(c.id);
            }
        }
        for e in gs.advance_day() {
            if let GameEvent::CompetitorLaunch { company, contract_name, .. } = e {
                if company == "DinoSoar" && flight_names.contains(&contract_name) {
                    launches_seen.insert(contract_name);
                }
            }
        }
    }
    for c in &gs.competitors[0].company.active_contracts {
        if c.campaign_id == Some(cid) {
            mission_ids.insert(c.id);
        }
    }

    assert_eq!(
        mission_ids.len(), 3,
        "seed {seed}: all 3 missions should have issued at some point, saw {}", mission_ids.len(),
    );
    assert!(
        gs.active_campaigns.iter().all(|c| c.id != cid),
        "seed {seed}: a campaign with all missions issued should have retired",
    );
    assert!(
        launches_seen.len() >= 2,
        "seed {seed}: expected at least 2 campaign launches to fire, got {launches_seen:?}",
    );
    assert!(
        gs.competitors[0].scheduled_launches.iter().all(|sl| !mission_ids.contains(&sl.contract_id)),
        "seed {seed}: none of this campaign's missions should still have a pending scheduled launch",
    );
}

// ---------------------------------------------------------------
// 5b. Dino eats the same program clause.
// ---------------------------------------------------------------

/// The program clause binds every winner: when DinoSoar lets a won
/// mission expire, the contract leaves its books, its scheduled launch
/// is dropped, its reputation takes the normal + program hit, and the
/// campaign records the strike with a public CampaignMissionMissed.
#[test]
fn dino_missed_mission_strikes_the_clause() {
    let seed = 7107;
    let mut gs = fresh_game(seed);

    let bid_deadline = gs.date.add_days(2);
    let cid = CampaignId(7010);
    let program = "Overreach Program";
    gs.active_campaigns.push(contested_campaign(7010, program, bid_deadline, gs.date));

    while gs.date < bid_deadline {
        gs.advance_day();
    }
    let events = gs.advance_day();
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::CampaignAwardedToCompetitor { .. })),
        "seed {seed}: expected DinoSoar to win {program} unopposed, got {events:?}",
    );

    // Force the issued mission overdue before its scheduled launch
    // day (launch = issue + 30-day lead, so an immediate deadline
    // always beats it).
    let mission_id = {
        let c = gs.competitors[0].company.active_contracts.iter_mut()
            .find(|c| c.campaign_id == Some(cid))
            .expect("Flight 1 should be on DinoSoar's books");
        c.deadline = gs.date;
        c.id
    };
    let expiry_before = gs.competitors[0].company.reputation.expiry_factor;
    let severity = gs.markets.iter()
        .find(|m| m.id == MARKET_GEO_COMSATS).unwrap().failure_severity;

    let events = gs.advance_day();

    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::CampaignMissionMissed { company, misses: 1, .. } if company == "DinoSoar",
        )),
        "seed {seed}: the strike should be public news, got {events:?}",
    );
    let expected_drop = gs.balance.reputation.expiry_penalty * severity
        * (1.0 + gs.balance.markets.campaign_miss_rep_penalty);
    let actual_drop = expiry_before - gs.competitors[0].company.reputation.expiry_factor;
    assert!(
        (actual_drop - expected_drop).abs() < 1e-9,
        "seed {seed}: dino's miss should cost normal + program hit \
         (expected {expected_drop}, got {actual_drop})",
    );
    assert!(
        !gs.competitors[0].company.active_contracts.iter().any(|c| c.id == mission_id),
        "seed {seed}: the expired mission should leave DinoSoar's books",
    );
    assert!(
        !gs.competitors[0].scheduled_launches.iter().any(|sl| sl.contract_id == mission_id),
        "seed {seed}: the expired mission's scheduled launch should be dropped",
    );
    assert_eq!(
        gs.active_campaigns.iter().find(|c| c.id == cid).unwrap().missions_missed, 1,
        "seed {seed}: the campaign should carry the strike",
    );
}

// ---------------------------------------------------------------
// 6. A competitor-won campaign survives save/load.
// ---------------------------------------------------------------

/// A campaign resolved in DinoSoar's favor — `Won { by_player: false }`
/// plus the resulting `ScheduledLaunch` in the competitor's own
/// bookkeeping — must round-trip exactly through save/load.
#[test]
fn competitor_won_campaign_save_roundtrip() {
    let seed = 7106;
    let mut gs = fresh_game(seed);

    let bid_deadline = gs.date.add_days(2);
    let cid = CampaignId(7006);
    let program = "Vanguard Series";
    gs.active_campaigns.push(contested_campaign(7006, program, bid_deadline, gs.date));

    while gs.date < bid_deadline {
        gs.advance_day();
    }
    let events = gs.advance_day();
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::CampaignAwardedToCompetitor { .. })),
        "seed {seed}: expected DinoSoar to win {program} unopposed, got {events:?}",
    );

    let before_campaign = gs.active_campaigns.iter().find(|c| c.id == cid)
        .expect("seed 7106: won campaign should still be tracked (not yet fully issued)")
        .clone();
    let before_scheduled_len = gs.competitors[0].scheduled_launches.len();

    let path = temp_save_path("roundtrip");
    rocket_tycoon::save::save_game(&gs, &path).expect("seed 7106: save should succeed");
    let loaded = rocket_tycoon::save::load_game(&path).expect("seed 7106: load should succeed");

    let after_campaign = loaded.active_campaigns.iter().find(|c| c.id == cid)
        .unwrap_or_else(|| panic!("seed {seed}: competitor-won campaign should survive round-trip"));
    assert_eq!(&before_campaign, after_campaign, "seed {seed}: campaign should round-trip exactly");
    assert_eq!(
        loaded.competitors[0].scheduled_launches.len(), before_scheduled_len,
        "seed {seed}: DinoSoar's scheduled_launches count should round-trip",
    );

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------
// 7. Resolution is deterministic given a fixed seed.
// ---------------------------------------------------------------

/// Running the exact same fixture twice from the same `GameState` seed
/// must produce the same winner and the same awarded price — the
/// jittered block bid is seeded per campaign id, not by wall-clock or
/// call order.
#[test]
fn resolution_is_deterministic() {
    fn run(seed: u64) -> (bool, f64) {
        let mut gs = fresh_game(seed);
        let bid_deadline = gs.date.add_days(2);
        let cid = CampaignId(7007);
        gs.active_campaigns.push(contested_campaign(7007, "Deterministic Fleet", bid_deadline, gs.date));

        let snapshot = gs.active_campaigns.last().unwrap().clone();
        let dino_expected = gs.competitors[0]
            .compute_block_bid(&snapshot, &gs.balance, &gs.seed)
            .expect("seed: DinoSoar should be willing to block-bid a fresh gto campaign");
        gs.place_campaign_bid(cid, dino_expected * 1.5).expect("player bid should be accepted");

        while gs.date < bid_deadline {
            gs.advance_day();
        }
        gs.advance_day();

        let campaign = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
        match &campaign.status {
            CampaignStatus::Won { by_player, .. } => (*by_player, campaign.payment_per_mission),
            other => panic!("expected Won, got {other:?}"),
        }
    }

    let seed = 7108;
    let (won1, price1) = run(seed);
    let (won2, price2) = run(seed);
    assert_eq!(won1, won2, "seed {seed}: winner should be identical across identical runs");
    assert_eq!(price1, price2, "seed {seed}: awarded price should be identical across identical runs");
}
