//! Scripted competitor companies (M3: DinoSoar).
//!
//! A competitor is a real [`Company`] — its rockets come off the same
//! `Manufacturing::advance_day` the player's do, with real orders,
//! floor space, teams, learning curves, and cost history. What's
//! scripted is everything *around* the company: a fixed catalog with
//! no R&D, a margin rule instead of a decision-making player, and
//! abstract launches (consume a real inventory rocket, roll the one
//! seeded flaw, no flight sim).
//!
//! Script parameters live in `balance_config::CompetitorConfig`; the
//! per-world realization (the seeded failure rate) happens here.

use rand::Rng;
use serde::{Serialize, Deserialize};

use crate::balance_config::BalanceConfig;
use crate::calendar::GameDate;
use crate::contract::{Contract, ContractId};
use crate::engine::{EngineDesign, EngineCycle, EngineId, PropellantFraction};
use crate::engine_project::{EngineProject, EngineDesignStatus, EngineProjectId, PropellantPreset};
use crate::flaw::{Flaw, FlawId, FlawConsequence, FlawTrigger};
use crate::game_state::Company;
use crate::manufacturing::InventoryRocket;
use crate::propellant::Propellant;
use crate::rocket::{RocketDesign, RocketDesignId};
use crate::rocket_project::{RocketProject, RocketDesignStatus, RocketProjectId};
use crate::seed::GameSeed;
use crate::stage::{Fairing, Stage, StageId};

/// A launch DinoSoar has committed to: the awarded contract and the
/// day the rocket leaves the pad. The reservation (award → launch)
/// is what keeps free stock honest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledLaunch {
    pub contract_id: ContractId,
    pub launch_date: GameDate,
}

/// A scripted competitor: a real company plus the state the script
/// needs. Serialized inside `GameState`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Competitor {
    pub company: Company,
    /// The catalog vehicle's project (its only rocket project).
    pub rocket_project_id: RocketProjectId,
    pub design_id: RocketDesignId,
    /// Per-flight loss-of-vehicle chance realized from the world seed.
    /// Mirrored in the design's single flaw; kept here so the script
    /// never has to dig it back out of a flaw list.
    pub failure_rate: f64,
    /// Awarded contracts awaiting their launch day.
    pub scheduled_launches: Vec<ScheduledLaunch>,
}

impl Competitor {
    /// Rockets in inventory not yet reserved by an awarded, unflown
    /// contract.
    pub fn free_stock(&self) -> u32 {
        let built = self.company.manufacturing.inventory.rocket_count(self.rocket_project_id) as u32;
        built.saturating_sub(self.scheduled_launches.len() as u32)
    }

    /// Marginal cost of the catalog vehicle: mean of the last few real
    /// builds, falling back to the configured catalog estimate until
    /// manufacturing has produced one.
    pub fn marginal_cost(&self, balance: &BalanceConfig) -> f64 {
        let history = self.company.rocket_cost_history.get(&self.design_id);
        match history {
            Some(h) if !h.is_empty() => {
                let recent = &h[h.len().saturating_sub(5)..];
                recent.iter().sum::<f64>() / recent.len() as f64
            }
            _ => balance.competitor.catalog_cost,
        }
    }

    /// Whether the catalog vehicle can serve a mission at all
    /// (destination in the capability table, payload within it).
    pub fn can_lift(&self, destination: &str, payload_kg: f64, balance: &BalanceConfig) -> bool {
        balance.competitor.capability.iter().any(|cap| {
            cap.location_id == destination && payload_kg <= cap.max_payload_kg
        })
    }

    /// The one margin-pricing rule behind every scripted bid, or None
    /// when the competitor declines (can't lift it, or no free stock —
    /// the same readiness gate the player's rules get).
    ///
    /// bid = marginal cost × margin × margin_factor, where the margin
    /// relaxes from `margin_max` (one rocket left) toward `margin_min`
    /// as free stock grows, jittered per `jitter_key` from the world
    /// seed, and never below `bid_floor`.
    fn scripted_bid(
        &self,
        destination: &str,
        payload_kg: f64,
        jitter_key: &str,
        margin_factor: f64,
        balance: &BalanceConfig,
        seed: &GameSeed,
    ) -> Option<f64> {
        let cfg = &balance.competitor;
        if !self.can_lift(destination, payload_kg, balance) {
            return None;
        }
        let free = self.free_stock();
        if free == 0 {
            return None;
        }
        let margin =
            (cfg.margin_min + (cfg.margin_max - cfg.margin_min) / free as f64) * margin_factor;
        let mut bid = self.marginal_cost(balance) * margin;
        let mut rng = seed.world_query(jitter_key);
        let u: f64 = rng.gen();
        bid *= 1.0 + cfg.bid_jitter * (2.0 * u - 1.0);
        bid = bid.max(cfg.bid_floor);
        Some((bid / 10_000.0).round() * 10_000.0)
    }

    /// The scripted sealed bid for a single solicitation.
    pub fn compute_bid(&self, contract: &Contract, balance: &BalanceConfig, seed: &GameSeed) -> Option<f64> {
        self.scripted_bid(
            &contract.destination,
            contract.payload_kg,
            &format!("dino_bid_{}", contract.id.0),
            1.0,
            balance,
            seed,
        )
    }

    /// The scripted sealed block bid for a campaign (one price per
    /// mission): the single-bid rule with the configured volume
    /// discount on the margin — an incumbent prices guaranteed volume
    /// slightly keener — jittered per campaign. The bid floor still
    /// applies, so small-payload programs stay priced out.
    pub fn compute_block_bid(
        &self,
        campaign: &crate::contract::Campaign,
        balance: &BalanceConfig,
        seed: &GameSeed,
    ) -> Option<f64> {
        self.scripted_bid(
            &campaign.destination,
            campaign.payload_kg,
            &format!("dino_block_bid_{}", campaign.id.0),
            1.0 - balance.competitor.block_discount,
            balance,
            seed,
        )
    }
}

/// Realize DinoSoar for a new world: seeded reliability, a mature
/// company with production lines and starting stock, and the fixed
/// heavy-lift catalog injected as ready-to-build (Testing) projects.
pub fn realize_dinosoar(seed: &GameSeed, balance: &BalanceConfig) -> Competitor {
    let cfg = &balance.competitor;

    // Seeded reliability: u^skew keeps most worlds near the base rate
    // and makes a battered ~95% DinoSoar rare.
    let mut rng = seed.world_query("competitor_dinosoar");
    let u: f64 = rng.gen();
    let failure_rate = cfg.failure_base + cfg.failure_spread * u.powf(cfg.failure_skew);

    let mut company = Company::new(cfg.name.clone(), cfg.starting_money, seed, balance);
    for i in 0..cfg.production_lines {
        company.hire_manufacturing_team(format!("Line {}", i + 1), balance);
    }
    company.manufacturing.floor_space.total_units = cfg.floor_space;

    // Catalog engines: injected directly in Testing, no flaws of
    // their own (the vehicle's whole failure story is the one rocket
    // flaw below). Numbers are Delta-IV-parody flavor; what matters
    // downstream is mass/complexity, which drive real build work and
    // material cost.
    let booster_engine = EngineDesign {
        id: EngineId(20_001),
        name: "TR-68 Thunderlizard".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 3_140_000.0,
        mass_kg: 6_600.0,
        isp_s: 386.0,
        exit_pressure_pa: 60_000.0,
        needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.86 },
            PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.14 },
        ],
        power_draw_w: 0.0,
    };
    let upper_engine = EngineDesign {
        id: EngineId(20_002),
        name: "DL-10 Dactyl".into(),
        cycle: EngineCycle::Expander,
        thrust_n: 110_000.0,
        mass_kg: 300.0,
        isp_s: 462.0,
        exit_pressure_pa: 5_000.0,
        needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.83 },
            PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.17 },
        ],
        power_draw_w: 0.0,
    };

    for (design, complexity) in [(booster_engine.clone(), 12u32), (upper_engine.clone(), 8u32)] {
        let project_id = EngineProjectId(company.next_project_id);
        company.next_project_id += 1;
        company.engine_projects.push(EngineProject {
            project_id,
            design,
            preset: PropellantPreset::Hydrolox,
            scale: 1.0,
            status: EngineDesignStatus::Testing { work_completed: 0.0 },
            flaws: Vec::new(),
            revision: 0,
            teams_assigned: 0,
            complexity,
            nre_cost: 0.0,
            improvements: Vec::new(),
            cumulative_testing_work: 0.0,
            tech_deficiency_ids: Vec::new(),
            technology_id: None,
        });
        // Mature product line: the learning curve starts well down.
        let ep_id = company.engine_projects.last().unwrap().project_id;
        company.engine_build_counts.insert(ep_id, cfg.prior_builds);
    }

    let design_id = RocketDesignId(20_001);
    let design = RocketDesign {
        id: design_id,
        name: "Brontosaur IV".into(),
        stage_groups: vec![
            vec![Stage {
                id: StageId(20_001),
                name: "Common Booster Core".into(),
                engine: booster_engine,
                engine_count: 1,
                propellant_mass_kg: 200_000.0,
                structural_mass_kg: 26_000.0,
                fairing: None,
                power_sources: Vec::new(),
            }],
            vec![Stage {
                id: StageId(20_002),
                name: "Cryo Upper".into(),
                engine: upper_engine,
                engine_count: 1,
                propellant_mass_kg: 27_000.0,
                structural_mass_kg: 3_500.0,
                fairing: Some(Fairing { mass_kg: 2_500.0, diameter_m: 5.1 }),
                power_sources: Vec::new(),
            }],
        ],
    };

    // Exactly one permanent loss-of-vehicle flaw carrying the seeded
    // failure rate. Never discovered, never revised — DinoSoar does
    // no R&D.
    let flaw = Flaw {
        id: FlawId(company.next_flaw_id),
        description: "Booster core separation ordnance defect".into(),
        consequence: FlawConsequence::StageLoss,
        activation_chance: failure_rate,
        discovery_probability: 0.0,
        discovered: false,
        trigger: FlawTrigger::PerFlight,
    };
    company.next_flaw_id += 1;

    let mut project = RocketProject::new(
        RocketProjectId(company.next_rocket_project_id),
        design,
        balance,
    );
    company.next_rocket_project_id += 1;
    project.status = RocketDesignStatus::Testing { work_completed: 0.0 };
    project.flaws = vec![flaw.clone()];
    let rocket_project_id = project.project_id;
    let rocket_name = project.design.name.clone();
    company.rocket_projects.push(project);

    company.rocket_build_counts.insert(design_id, cfg.prior_builds);
    company.auto_build_targets.insert(rocket_project_id, cfg.auto_build_target);

    // The incumbent starts with vehicles on the shelf, valued at the
    // catalog estimate (no build history yet for these).
    for _ in 0..cfg.initial_stock {
        let item_id = company.manufacturing.next_inventory_id();
        company.manufacturing.inventory.rockets.push(InventoryRocket {
            item_id,
            rocket_project_id,
            design_id,
            rocket_name: rocket_name.clone(),
            build_cost: cfg.catalog_cost,
            revision: 0,
            rocket_flaws: vec![flaw.clone()],
        });
    }

    Competitor {
        company,
        rocket_project_id,
        design_id,
        failure_rate,
        scheduled_launches: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::GameSeed;

    fn dino(seed: u64) -> Competitor {
        realize_dinosoar(&GameSeed::new(seed), &BalanceConfig::default())
    }

    #[test]
    fn test_realization_is_deterministic() {
        let a = dino(42);
        let b = dino(42);
        assert_eq!(a.failure_rate, b.failure_rate);
        assert_eq!(
            a.company.manufacturing.inventory.rockets.len(),
            b.company.manufacturing.inventory.rockets.len(),
        );
    }

    #[test]
    fn test_failure_rate_distribution_is_skewed() {
        let cfg = BalanceConfig::default();
        let rates: Vec<f64> = (1..=200).map(|s| dino(s).failure_rate).collect();
        for &r in &rates {
            assert!(r >= cfg.competitor.failure_base);
            assert!(r <= cfg.competitor.failure_base + cfg.competitor.failure_spread + 1e-12);
        }
        let below_1pct = rates.iter().filter(|&&r| r <= 0.011).count();
        let above_3pct = rates.iter().filter(|&&r| r >= 0.03).count();
        assert!(
            below_1pct >= 140,
            "only {below_1pct}/200 worlds have a ≥98.9% reliable DinoSoar; the norm should be 99%+",
        );
        assert!(
            above_3pct <= 20,
            "{above_3pct}/200 worlds have a ≤97% DinoSoar; sub-97% should be rare",
        );
        assert!(above_3pct >= 1, "a battered DinoSoar should exist somewhere in 200 seeds");
    }

    #[test]
    fn test_company_is_ready_to_build() {
        let d = dino(7);
        let cfg = BalanceConfig::default();
        assert_eq!(
            d.company.manufacturing_teams.len() as u32,
            cfg.competitor.production_lines,
        );
        assert_eq!(d.company.manufacturing.floor_space.total_units, cfg.competitor.floor_space);
        assert_eq!(d.company.engine_projects.len(), 2);
        assert_eq!(d.company.rocket_projects.len(), 1);
        let rp = &d.company.rocket_projects[0];
        assert!(matches!(rp.status, RocketDesignStatus::Testing { .. }));
        assert_eq!(rp.flaws.len(), 1, "exactly one permanent flaw");
        assert_eq!(rp.flaws[0].activation_chance, d.failure_rate);
        assert_eq!(
            d.company.auto_build_targets.get(&d.rocket_project_id),
            Some(&cfg.competitor.auto_build_target),
        );
        assert_eq!(d.free_stock(), cfg.competitor.initial_stock);
    }

    #[test]
    fn test_bid_margin_rises_as_stock_shrinks() {
        let cfg = BalanceConfig::default();
        let seed = GameSeed::new(9);
        let mut d = realize_dinosoar(&seed, &cfg);
        let contract = Contract {
            destination: "gto".into(),
            payload_kg: 5_000.0,
            ..crate::contract::test_support::solicitation_fixture()
        };
        let scarce = d.compute_bid(&contract, &cfg, &seed).expect("bids with stock");
        // Deepen the shelf: more stock, thinner margin.
        for _ in 0..6 {
            let item_id = d.company.manufacturing.next_inventory_id();
            let template = d.company.manufacturing.inventory.rockets[0].clone();
            d.company.manufacturing.inventory.rockets.push(InventoryRocket {
                item_id, ..template
            });
        }
        let flush = d.compute_bid(&contract, &cfg, &seed).expect("bids with stock");
        assert!(
            flush < scarce,
            "bid should fall as free stock grows (scarce ${scarce:.0} vs flush ${flush:.0})",
        );
        assert!(flush >= cfg.competitor.bid_floor);
    }

    #[test]
    fn test_block_bid_is_discounted_single_bid() {
        // With jitter zeroed the two rules differ only by the margin
        // discount, so the relationship is exact.
        let mut cfg = BalanceConfig::default();
        cfg.competitor.bid_jitter = 0.0;
        let seed = GameSeed::new(13);
        let d = realize_dinosoar(&seed, &cfg);
        let contract = Contract {
            destination: "gto".into(),
            payload_kg: 5_000.0,
            ..crate::contract::test_support::solicitation_fixture()
        };
        let campaign = crate::contract::Campaign {
            id: crate::contract::CampaignId(1),
            name: "Discount Probe".into(),
            market_id: crate::contract::MarketId(1),
            destination: "gto".into(),
            destination_display: "GTO".into(),
            payload_kg: 5_000.0,
            payment_per_mission: 200_000_000.0,
            missions_total: 3,
            missions_issued: 0,
            missions_missed: 0,
            next_issue_date: GameDate::default_start(),
            interval_days: 30,
            status: crate::contract::CampaignStatus::Soliciting {
                bid_deadline: GameDate::default_start(),
                budget_ceiling_per_mission: 240_000_000.0,
                player_bid: None,
            },
        };
        let single = d.compute_bid(&contract, &cfg, &seed).expect("single bid");
        let block = d.compute_block_bid(&campaign, &cfg, &seed).expect("block bid");
        let expected = ((single * (1.0 - cfg.competitor.block_discount)) / 10_000.0).round() * 10_000.0;
        assert!(
            (block - expected).abs() < 10_000.0 + 1e-6,
            "block bid ${block:.0} should be the single bid ${single:.0} with a \
             {}% keener margin (expected ~${expected:.0})",
            cfg.competitor.block_discount * 100.0,
        );
        assert!(block >= cfg.competitor.bid_floor);

        // The discount never takes a block bid below the floor: even a
        // total margin collapse stays floored.
        let mut floor_cfg = cfg.clone();
        floor_cfg.competitor.margin_min = 1.0;
        floor_cfg.competitor.margin_max = 1.0;
        floor_cfg.competitor.block_discount = 0.9;
        let floored = d.compute_block_bid(&campaign, &floor_cfg, &seed).expect("floored bid");
        assert_eq!(floored, floor_cfg.competitor.bid_floor);
    }

    #[test]
    fn test_declines_without_stock_or_capability() {
        let cfg = BalanceConfig::default();
        let seed = GameSeed::new(11);
        let mut d = realize_dinosoar(&seed, &cfg);
        let contract = Contract {
            destination: "gto".into(),
            payload_kg: 5_000.0,
            ..crate::contract::test_support::solicitation_fixture()
        };

        // Reserve every rocket: no free stock, no bid.
        let stock = d.company.manufacturing.inventory.rockets.len();
        for i in 0..stock {
            d.scheduled_launches.push(ScheduledLaunch {
                contract_id: ContractId(90_000 + i as u64),
                launch_date: GameDate::default_start(),
            });
        }
        assert_eq!(d.compute_bid(&contract, &cfg, &seed), None);
        d.scheduled_launches.clear();

        // Payload beyond the capability table: no bid.
        let heavy = Contract {
            destination: "gto".into(),
            payload_kg: 1_000_000.0,
            ..crate::contract::test_support::solicitation_fixture()
        };
        assert_eq!(d.compute_bid(&heavy, &cfg, &seed), None);

        // Destination it doesn't serve: no bid.
        let weird = Contract {
            destination: "phobos".into(),
            payload_kg: 500.0,
            ..crate::contract::test_support::solicitation_fixture()
        };
        assert_eq!(d.compute_bid(&weird, &cfg, &seed), None);
    }
}
