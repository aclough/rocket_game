//! Markets: what they pay, how often they solicit, how bids are
//! scored, and the monthly contract generation.

use super::*;

/// How sensitive a market is to economic cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EconomySensitivity {
    /// Unaffected (government/military).
    None,
    /// Slightly affected.
    Low,
    /// Directly tracks economy.
    Moderate,
    /// Amplified swings.
    High,
}

impl EconomySensitivity {
    /// Apply economy modifier with appropriate sensitivity.
    pub fn apply(&self, economy_modifier: f64) -> f64 {
        match self {
            EconomySensitivity::None => 1.0,
            EconomySensitivity::Low => 1.0 + (economy_modifier - 1.0) * 0.3,
            EconomySensitivity::Moderate => economy_modifier,
            EconomySensitivity::High => 1.0 + (economy_modifier - 1.0) * 1.5,
        }
    }
}

/// A destination within a market that contracts can target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketDestination {
    pub location_id: String,
    pub display_name: String,
    pub min_payload_kg: f64,
    pub max_payload_kg: f64,
    pub rate_per_kg: f64,
    /// Relative weight for random selection among destinations in this market.
    pub weight: f64,
}

/// An active modifier on a market (from events, competition, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketModifier {
    /// Unique key — checked for duplicates when adding.
    pub id: String,
    /// Human-readable description shown in market info.
    pub description: String,
    /// Multiplier to base volume (1.0 = no change).
    pub volume_mult: f64,
    /// Multiplier to payment rates (1.0 = no change).
    pub rate_mult: f64,
    /// When this modifier expires (None = permanent).
    pub end_date: Option<GameDate>,
    /// Per-destination volume multipliers, keyed by `location_id`.
    /// Absent means 1.0.
    ///
    /// A debris cascade is an orbit problem, not a market problem: it
    /// wrecks LEO and SSO and barely touches GEO. A market-wide
    /// `volume_mult` can't say that, so this scales one destination at a
    /// time and the two consequences fall out together — the market's
    /// total volume drops by the affected destinations' share of its
    /// weight (see `Market::destination_share`), and what contracts
    /// remain skew toward the orbits that still work (see
    /// `pick_destination`).
    #[serde(default)]
    pub destination_volume_mult: Vec<(String, f64)>,
    /// Added to the market's `rep_target` while active. Negative makes
    /// the customer less choosy — a wartime buyer needs lift more than
    /// it needs a spotless record.
    #[serde(default)]
    pub rep_target_delta: f64,
}

impl Default for MarketModifier {
    /// A modifier that changes nothing — construct with
    /// `MarketModifier { id, description, volume_mult: 0.6,
    /// ..Default::default() }` and name only the fields you mean.
    fn default() -> Self {
        MarketModifier {
            id: String::new(),
            description: String::new(),
            volume_mult: 1.0,
            rate_mult: 1.0,
            end_date: None,
            destination_volume_mult: Vec::new(),
            rep_target_delta: 0.0,
        }
    }
}

/// When a market's contracts arrive: evenly, in clumps, or in
/// batches. Every variant conserves long-run volume — cadence
/// reshapes *when* contracts appear, never how many.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[derive(Default)]
pub enum Cadence {
    /// Even monthly flow (the pre-cadence behavior).
    #[default]
    Steady,
    /// Irregular: a month goes quiet with probability `quiet_chance`;
    /// active months run at boosted volume to compensate.
    Lumpy { quiet_chance: f64 },
    /// Batchy: quiet most months, then a burst month (probability
    /// `burst_chance`) generates the accumulated volume at once.
    Burst { burst_chance: f64 },
}


impl Cadence {
    /// Roll this month's volume multiplier. Each variant has
    /// expectation 1.0, so long-run volume is conserved.
    pub fn monthly_multiplier(&self, rng: &mut StdRng) -> f64 {
        match *self {
            Cadence::Steady => 1.0,
            Cadence::Lumpy { quiet_chance } => {
                if rng.gen::<f64>() < quiet_chance {
                    0.0
                } else {
                    1.0 / (1.0 - quiet_chance)
                }
            }
            Cadence::Burst { burst_chance } => {
                if rng.gen::<f64>() < burst_chance {
                    1.0 / burst_chance
                } else {
                    0.0
                }
            }
        }
    }
}

/// A launch market that generates contracts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub id: MarketId,
    pub name: String,
    pub description: String,
    pub active: bool,
    /// Contracts per month before modifiers.
    pub base_volume: f64,
    pub destinations: Vec<MarketDestination>,
    /// The reputation this market's customers expect (M3 award
    /// scoring): well below it your bids score near zero on the
    /// reputation axis, well above it saturates. May be negative
    /// (cubesat customers will fly with anyone). Replaces the old
    /// `min_reputation` visibility gate — every active market's
    /// solicitations are visible regardless of reputation.
    #[serde(alias = "min_reputation")]
    pub rep_target: f64,
    /// Award-scoring weight on price (budget / bid).
    #[serde(default = "default_w_cost")]
    pub w_cost: f64,
    /// Award-scoring weight on the logistic reputation factor.
    #[serde(default = "default_w_rep")]
    pub w_rep: f64,
    /// Budget ceiling as a multiple of the reference payment (>= 1.0
    /// so a reference-priced bid is always within budget).
    #[serde(default = "default_budget_tolerance")]
    pub budget_tolerance: f64,
    pub economy_sensitivity: EconomySensitivity,
    pub name_prefixes: Vec<String>,
    pub modifiers: Vec<MarketModifier>,
    /// Compounding annual volume growth rate (0.05 = +5%/year),
    /// drawn per seed at realization.
    #[serde(default)]
    pub annual_growth: f64,
    /// When this market became active; growth compounds from here.
    /// None until activation (and on pre-growth saves).
    #[serde(default)]
    pub activation_date: Option<GameDate>,
    /// Per-market contract deadline window in days from issue;
    /// None falls back to the global `MarketsConfig` window.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub deadline_days: Option<(u32, u32)>,
    /// Multiplier on reputation penalties for failures and expiries
    /// involving this market's contracts (1.0 = baseline; crewed
    /// markets are much less forgiving, government science more so).
    #[serde(default = "default_severity")]
    pub failure_severity: f64,
    /// How this market's contracts arrive over time.
    #[serde(default)]
    pub cadence: Cadence,
    /// Fractional volume carried between months (Steady cadence
    /// only): monthly count = floor of accumulated volume, so
    /// "steady" is literally steady. This is what makes the year-1
    /// opening floor deterministic instead of statistical — a run of
    /// unlucky draws can never starve a Steady market's year
    /// (opening-floor markets are required to be Steady).
    #[serde(default)]
    pub volume_accumulator: f64,
}

fn default_severity() -> f64 {
    1.0
}

fn default_w_cost() -> f64 {
    0.7
}

fn default_w_rep() -> f64 {
    0.3
}

fn default_budget_tolerance() -> f64 {
    1.2
}

/// Logistic reputation factor for award scoring: 0.5 at the market's
/// target, saturating toward 1 well above it and toward 0 well below.
/// Handles negative reputations and negative targets with no special
/// cases. `rep_scale` (global config) sets how wide "near target" is.
pub fn rep_factor(reputation: f64, rep_target: f64, rep_scale: f64) -> f64 {
    1.0 / (1.0 + ((rep_target - reputation) / rep_scale).exp())
}

/// Score a sealed bid for award resolution. Higher wins. Bids above
/// the budget ceiling must be rejected by the caller before scoring.
pub fn bid_score(
    bid: f64,
    budget_ceiling: f64,
    reputation: f64,
    market: &Market,
    rep_scale: f64,
) -> f64 {
    market.w_cost * (budget_ceiling / bid)
        + market.w_rep * rep_factor(reputation, market.effective_rep_target(), rep_scale)
}

impl Market {
    /// Compounding growth multiplier accumulated since activation
    /// (1.0 before activation or with zero growth).
    pub fn growth_factor(&self, current_date: GameDate) -> f64 {
        match self.activation_date {
            Some(activated) if current_date > activated => {
                let years = activated.days_until(&current_date) as f64 / DAYS_PER_YEAR;
                (1.0 + self.annual_growth).powf(years)
            }
            _ => 1.0,
        }
    }

    /// Effective volume after growth and all modifiers.
    pub fn effective_volume(&self, economy_modifier: f64, current_date: GameDate) -> f64 {
        let econ = self.economy_sensitivity.apply(economy_modifier);
        let mod_mult: f64 = self.modifiers.iter().map(|m| m.volume_mult).product();
        self.base_volume * self.growth_factor(current_date) * mod_mult * econ
            * self.destination_share()
    }

    /// Combined per-destination multiplier for one location — the product
    /// across every active modifier. 1.0 when nothing touches it.
    pub fn destination_mult(&self, location_id: &str) -> f64 {
        self.modifiers.iter()
            .flat_map(|m| m.destination_volume_mult.iter())
            .filter(|(loc, _)| loc == location_id)
            .map(|(_, mult)| *mult)
            .product()
    }

    /// How much of this market's volume survives its per-destination
    /// multipliers: the weight-weighted mean of them.
    ///
    /// Suppressing an orbit has to remove those launches rather than
    /// redistribute them, so the market's total falls by exactly the
    /// share of its weight that sat on the affected destinations. A
    /// market with no LEO business is untouched by a LEO catastrophe;
    /// one that is all LEO loses everything.
    pub fn destination_share(&self) -> f64 {
        let total: f64 = self.destinations.iter().map(|d| d.weight).sum();
        if total <= 0.0 {
            return 1.0;
        }
        let surviving: f64 = self.destinations.iter()
            .map(|d| d.weight * self.destination_mult(&d.location_id))
            .sum();
        surviving / total
    }

    /// The reputation bar as it stands today, including any modifier.
    pub fn effective_rep_target(&self) -> f64 {
        self.rep_target + self.modifiers.iter().map(|m| m.rep_target_delta).sum::<f64>()
    }

    /// Effective rate multiplier from all modifiers.
    pub fn rate_multiplier(&self, economy_modifier: f64) -> f64 {
        let econ = self.economy_sensitivity.apply(economy_modifier);
        let mod_mult: f64 = self.modifiers.iter().map(|m| m.rate_mult).product();
        mod_mult * econ
    }

    /// Add a modifier, checking for duplicates by id.
    pub fn add_modifier(&mut self, modifier: MarketModifier) {
        if !self.modifiers.iter().any(|m| m.id == modifier.id) {
            self.modifiers.push(modifier);
        }
    }

    /// Remove expired modifiers.
    pub fn expire_modifiers(&mut self, current_date: GameDate) {
        self.modifiers.retain(|m| {
            m.end_date.is_none_or(|end| current_date < end)
        });
    }
}

/// Generate contracts for a single market for one month. Every
/// active market generates regardless of player reputation — the
/// reputation question moved from visibility to award scoring (M3).
pub fn generate_market_contracts(
    market: &mut Market,
    rng: &mut StdRng,
    next_contract_id: &mut crate::id::IdAllocator<ContractId>,
    current_date: GameDate,
    economy_modifier: f64,
    markets_cfg: &MarketsConfig,
) -> Vec<Contract> {
    if !market.active {
        return Vec::new();
    }

    let effective_volume = market.effective_volume(economy_modifier, current_date);
    let count = match market.cadence {
        // Steady is literally steady: volume accumulates and each
        // whole contract is issued as soon as it's earned. No draw —
        // a Steady market's monthly counts are a deterministic
        // function of its volume curve.
        Cadence::Steady => {
            market.volume_accumulator += effective_volume;
            let whole = market.volume_accumulator.floor();
            market.volume_accumulator -= whole;
            whole as u32
        }
        // Lumpy/Burst keep their randomness — that's their character.
        _ => {
            let cadence_mult = market.cadence.monthly_multiplier(rng);
            (effective_volume * cadence_mult + rng.gen::<f64>()) as u32
        }
    };
    let rate_mult = market.rate_multiplier(economy_modifier);

    let mut contracts = Vec::new();
    for _ in 0..count {
        if let Some(c) = generate_single_contract(
            market, rng, next_contract_id, current_date, rate_mult, markets_cfg,
        ) {
            contracts.push(c);
        }
    }
    contracts
}

/// Pick a destination by weight (None if the market has no positive
/// weights).
pub(super) fn pick_destination<'a>(market: &'a Market, rng: &mut StdRng) -> Option<&'a MarketDestination> {
    // Weights are scaled by any per-destination modifier, so a suppressed
    // orbit gets proportionally fewer of the contracts that remain. The
    // matching drop in how many there are is in `destination_share`.
    let effective = |d: &MarketDestination| d.weight * market.destination_mult(&d.location_id);
    let total_weight: f64 = market.destinations.iter().map(effective).sum();
    if total_weight <= 0.0 {
        return None;
    }
    let mut roll = rng.gen::<f64>() * total_weight;
    let mut dest = market.destinations.first()?;
    for d in &market.destinations {
        roll -= effective(d);
        if roll <= 0.0 {
            dest = d;
            break;
        }
    }
    Some(dest)
}

fn generate_single_contract(
    market: &Market,
    rng: &mut StdRng,
    next_contract_id: &mut crate::id::IdAllocator<ContractId>,
    current_date: GameDate,
    rate_mult: f64,
    markets_cfg: &MarketsConfig,
) -> Option<Contract> {
    if market.destinations.is_empty() || market.name_prefixes.is_empty() {
        return None;
    }

    let dest = pick_destination(market, rng)?;

    let payload_kg = rng.gen_range(dest.min_payload_kg..=dest.max_payload_kg);
    let payload_kg = (payload_kg / 100.0).round() * 100.0;
    let payload_kg = payload_kg.max(dest.min_payload_kg);

    let base_payment = payload_kg * dest.rate_per_kg;
    let variance = rng.gen_range(markets_cfg.payment_variance_min..=markets_cfg.payment_variance_max);
    let payment = round_price(base_payment * variance * rate_mult);

    let (deadline_min, deadline_max) = market.deadline_days
        .unwrap_or((markets_cfg.deadline_min_days, markets_cfg.deadline_max_days));
    let deadline_days = rng.gen_range(deadline_min..=deadline_max);
    let deadline = current_date.add_days(deadline_days);

    let prefix = &market.name_prefixes[rng.gen_range(0..market.name_prefixes.len())];
    let name = format!("{} to {}", prefix, dest.display_name);

    let id = next_contract_id.mint();

    Some(Contract {
        id,
        name,
        destination: dest.location_id.clone(),
        payload_kg,
        payment,
        deadline,
        status: ContractStatus::Available,
        market_id: market.id,
        campaign_id: None,
        bid_deadline: Some(current_date.add_days(markets_cfg.bid_window_days)),
        budget_ceiling: payment * market.budget_tolerance,
        player_bid: None,
    })
}

// ==========================================
// Anchor-customer campaigns (M2)
// ==========================================

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;


    fn make_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    fn mcfg() -> MarketsConfig {
        MarketsConfig::default()
    }

    #[test]
    fn test_initial_markets_count() {
        let markets = initial_markets();
        assert_eq!(markets.len(), 3);
        assert!(markets.iter().all(|m| m.active));
    }

    #[test]
    fn test_event_markets_inactive() {
        let markets = event_market_templates();
        assert!(markets.iter().all(|m| !m.active));
    }

    #[test]
    fn test_generation_ignores_reputation() {
        // M3: reputation gates awards via scoring, not visibility —
        // every active market generates solicitations.
        let markets = initial_markets();
        let mut rng = make_rng();
        let date = GameDate::new(2001, 1, 1);
        let mut next_id = crate::id::IdAllocator::<ContractId>::starting_at(1);

        let mut geo = markets.iter().find(|m| m.id == MARKET_GEO_COMSATS).unwrap().clone();
        let cs = generate_market_contracts(&mut geo, &mut rng, &mut next_id, date, 1.0, &mcfg());
        // GEO base_volume 1.5: generates at least one most months.
        assert!(
            !cs.is_empty(),
            "GEO (rep_target 50) must generate solicitations regardless of reputation",
        );
        assert!(cs.iter().all(|c| c.market_id == MARKET_GEO_COMSATS));
    }

    #[test]
    fn test_generated_contracts_are_solicitations() {
        let markets = initial_markets();
        let mut rng = make_rng();
        let date = GameDate::new(2001, 1, 1);
        let mut next_id = crate::id::IdAllocator::<ContractId>::starting_at(1);
        let cfg = mcfg();

        let mut geo = markets.iter().find(|m| m.id == MARKET_GEO_COMSATS).unwrap().clone();
        let cs = generate_market_contracts(&mut geo, &mut rng, &mut next_id, date, 1.0, &cfg);
        for c in &cs {
            assert!(c.is_solicitation());
            assert_eq!(c.bid_deadline, Some(date.add_days(cfg.bid_window_days)));
            assert!(
                (c.budget_ceiling - c.payment * geo.budget_tolerance).abs() < 1e-6,
                "ceiling must be reference payment x budget_tolerance",
            );
            assert!(c.player_bid.is_none());
        }
    }

    #[test]
    fn test_rep_factor_shape() {
        let scale = 10.0;
        // At target: exactly 0.5.
        assert!((rep_factor(50.0, 50.0, scale) - 0.5).abs() < 1e-12);
        // Well above saturates toward 1; well below decays toward 0.
        assert!(rep_factor(90.0, 50.0, scale) > 0.98);
        assert!(rep_factor(10.0, 50.0, scale) < 0.02);
        // Monotonic in reputation.
        assert!(rep_factor(55.0, 50.0, scale) > rep_factor(45.0, 50.0, scale));
        // Negative targets and negative reputations need no special
        // cases: a rep-0 startup is comfortably above a -10 target.
        assert!(rep_factor(0.0, -10.0, scale) > 0.7);
        assert!(rep_factor(-30.0, -10.0, scale) < 0.2);
    }

    #[test]
    fn test_bid_score_tradeoffs() {
        let market = &initial_markets()[0]; // GEO: w_cost 0.6, w_rep 0.4
        let scale = 10.0;
        let ceiling = 12_000_000.0;
        // Lower bid scores higher at equal reputation.
        assert!(
            bid_score(9e6, ceiling, 50.0, market, scale)
                > bid_score(11e6, ceiling, 50.0, market, scale),
        );
        // Higher reputation scores higher at equal bid.
        assert!(
            bid_score(1e7, ceiling, 60.0, market, scale)
                > bid_score(1e7, ceiling, 40.0, market, scale),
        );
        // A rep-heavy market forgives a higher bid more than a
        // cost-heavy one: the incumbent premium.
        let gov = &initial_markets()[1]; // gov science: 0.4/0.6
        let rideshare = &initial_markets()[2]; // rideshare: 0.8/0.2
        let premium_gov = bid_score(1.2e7, ceiling, 80.0, gov, scale)
            - bid_score(1e7, ceiling, 0.0, gov, scale);
        let premium_ride = bid_score(1.2e7, ceiling, 80.0, rideshare, scale)
            - bid_score(1e7, ceiling, 0.0, rideshare, scale);
        assert!(premium_gov > premium_ride);
    }

    #[test]
    fn test_economy_sensitivity() {
        assert_eq!(EconomySensitivity::None.apply(0.5), 1.0);
        assert_eq!(EconomySensitivity::None.apply(1.5), 1.0);

        let low = EconomySensitivity::Low.apply(0.5);
        assert!(low > 0.8 && low < 0.9, "Low sensitivity at 0.5x should be ~0.85, got {}", low);

        assert_eq!(EconomySensitivity::Moderate.apply(0.5), 0.5);
        assert_eq!(EconomySensitivity::Moderate.apply(1.5), 1.5);

        let high = EconomySensitivity::High.apply(0.5);
        assert!(high < 0.3, "High sensitivity at 0.5x should be ~0.25, got {}", high);
    }

    #[test]
    fn test_market_modifier_dedup() {
        let mut market = initial_markets().remove(0);
        market.add_modifier(MarketModifier {
            id: "test".into(), description: "Test".into(),
            volume_mult: 0.5, rate_mult: 1.0, end_date: None,
            ..Default::default()
        });
        market.add_modifier(MarketModifier {
            id: "test".into(), description: "Test duplicate".into(),
            volume_mult: 0.3, rate_mult: 1.0, end_date: None,
            ..Default::default()
        });
        assert_eq!(market.modifiers.len(), 1, "Should deduplicate by id");
    }

    #[test]
    fn test_modifier_affects_volume() {
        let mut market = initial_markets().remove(0); // GEO, base_volume 1.5
        let date = GameDate::new(2001, 1, 1);
        let vol_before = market.effective_volume(1.0, date);
        market.add_modifier(MarketModifier {
            id: "test".into(), description: "Test".into(),
            volume_mult: 0.5, rate_mult: 1.0, end_date: None,
            ..Default::default()
        });
        let vol_after = market.effective_volume(1.0, date);
        assert!((vol_after - vol_before * 0.5).abs() < 0.01);
    }

    /// Generate `months` of contracts for a synthetic market with the
    /// given cadence and return the per-month counts.
    fn monthly_counts(cadence: Cadence, months: u32) -> Vec<usize> {
        let mut market = initial_markets().remove(2); // Rideshare, rep 0
        market.base_volume = 1.0;
        market.cadence = cadence;
        let mut rng = make_rng();
        let mut next_id = crate::id::IdAllocator::<ContractId>::starting_at(1);
        let mut counts = Vec::new();
        for m in 0..months {
            let date = GameDate::new(2001 + m / 12, m % 12 + 1, 1);
            let cs = generate_market_contracts(
                &mut market, &mut rng, &mut next_id, date, 1.0, &mcfg(),
            );
            counts.push(cs.len());
        }
        counts
    }

    #[test]
    fn test_cadence_conserves_long_run_volume() {
        // Every cadence has a monthly multiplier with expectation 1.0,
        // so total contracts over N months must track base_volume * N.
        let months = 2400;
        for cadence in [
            Cadence::Steady,
            Cadence::Lumpy { quiet_chance: 0.4 },
            Cadence::Burst { burst_chance: 0.2 },
        ] {
            let total: usize = monthly_counts(cadence, months).iter().sum();
            let expected = months as f64; // base_volume 1.0
            let ratio = total as f64 / expected;
            assert!(
                (0.9..=1.1).contains(&ratio),
                "{cadence:?}: {total} contracts over {months} months, \
                 {:.0}% of expected — cadence must conserve volume",
                ratio * 100.0,
            );
        }
    }

    #[test]
    fn test_cadence_shapes_differ() {
        let months = 240;
        let zero_share = |counts: &[usize]| {
            counts.iter().filter(|&&c| c == 0).count() as f64 / counts.len() as f64
        };

        // Steady at volume 1.0 delivers every month.
        let steady = monthly_counts(Cadence::Steady, months);
        assert_eq!(zero_share(&steady), 0.0, "steady should never skip a month");

        // Lumpy skips roughly its quiet_chance share of months.
        let lumpy = monthly_counts(Cadence::Lumpy { quiet_chance: 0.4 }, months);
        let lumpy_zero = zero_share(&lumpy);
        assert!(
            (0.25..=0.55).contains(&lumpy_zero),
            "lumpy zero-month share {lumpy_zero:.2} not near quiet_chance 0.4",
        );

        // Burst is mostly quiet with occasional big months.
        let burst = monthly_counts(Cadence::Burst { burst_chance: 0.2 }, months);
        let burst_zero = zero_share(&burst);
        assert!(
            burst_zero >= 0.6,
            "burst zero-month share {burst_zero:.2} should be well above half",
        );
        let max_month = *burst.iter().max().unwrap();
        assert!(
            max_month >= 4,
            "burst peak month {max_month} should batch several contracts (5x volume)",
        );
    }

    #[test]
    fn test_growth_compounds_from_activation() {
        let mut market = initial_markets().remove(0);
        market.annual_growth = 0.10;

        // No activation date -> no growth.
        assert_eq!(market.growth_factor(GameDate::new(2005, 1, 1)), 1.0);

        market.activation_date = Some(GameDate::new(2001, 1, 1));
        // Before/at activation -> no growth.
        assert_eq!(market.growth_factor(GameDate::new(2000, 6, 1)), 1.0);
        assert_eq!(market.growth_factor(GameDate::new(2001, 1, 1)), 1.0);
        // Two years on -> ~1.1^2, within leap-day slop.
        let two_years = market.growth_factor(GameDate::new(2003, 1, 1));
        assert!(
            (two_years - 1.21).abs() < 0.01,
            "expected ~1.21 growth factor after 2 years at 10%, got {two_years}",
        );
        // Growth feeds effective_volume.
        let base = market.effective_volume(1.0, GameDate::new(2001, 1, 1));
        let grown = market.effective_volume(1.0, GameDate::new(2003, 1, 1));
        assert!((grown / base - two_years).abs() < 1e-9);
    }

    #[test]
    fn test_negative_growth_shrinks() {
        let mut market = initial_markets().remove(0);
        market.annual_growth = -0.02;
        market.activation_date = Some(GameDate::new(2001, 1, 1));
        let factor = market.growth_factor(GameDate::new(2011, 1, 1));
        assert!(
            factor < 1.0 && factor > 0.7,
            "expected mild decade-long decline at -2%/yr, got {factor}",
        );
    }

    #[test]
    fn test_expire_modifiers() {
        let mut market = initial_markets().remove(0);
        market.add_modifier(MarketModifier {
            id: "temp".into(), description: "Temp".into(),
            volume_mult: 0.5, rate_mult: 1.0,
            end_date: Some(GameDate::new(2005, 1, 1)),
            ..Default::default()
        });
        market.add_modifier(MarketModifier {
            id: "perm".into(), description: "Perm".into(),
            volume_mult: 0.8, rate_mult: 1.0, end_date: None,
            ..Default::default()
        });
        market.expire_modifiers(GameDate::new(2006, 1, 1));
        assert_eq!(market.modifiers.len(), 1);
        assert_eq!(market.modifiers[0].id, "perm");
    }

    #[test]
    fn test_contract_has_market_id() {
        let mut market = initial_markets()[2].clone(); // Rideshare
        let mut rng = make_rng();
        let mut next_id = crate::id::IdAllocator::<ContractId>::starting_at(1);
        let cs = generate_market_contracts(&mut market, &mut rng, &mut next_id, GameDate::new(2001, 1, 1), 1.0, &mcfg());
        for c in &cs {
            assert_eq!(c.market_id, MARKET_RIDESHARE);
        }
    }

    #[test]
    fn test_inactive_market_generates_nothing() {
        let mut market = event_market_templates()[0].clone(); // COTS, inactive
        let mut rng = make_rng();
        let mut next_id = crate::id::IdAllocator::<ContractId>::starting_at(1);
        let cs = generate_market_contracts(&mut market, &mut rng, &mut next_id, GameDate::new(2001, 1, 1), 1.0, &mcfg());
        assert!(cs.is_empty());
    }
}
