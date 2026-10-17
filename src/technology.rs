use rand::Rng;
use serde::{Serialize, Deserialize};

use crate::balance_config::TechnologyConfig;
use crate::seed::{GameSeed, WorldQuery};

/// Unique identifier for a technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TechnologyId(pub u64);

/// Unique identifier for a tech deficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TechDeficiencyId(pub u64);

/// What a tech deficiency does to a designed part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TechDeficiencyKind {
    /// Isp reduced by this fraction (e.g. 0.10 = -10%). Engine-only.
    IspPenalty(f64),
    /// Mass increased by this fraction (e.g. 0.15 = +15%).
    MassPenalty(f64),
    /// Thrust reduced by this fraction. Engine-only.
    ThrustPenalty(f64),
    /// Steady electrical output reduced by this fraction. Reactor-only.
    PowerPenalty(f64),
    /// Adds to effective complexity (more flaws, harder testing).
    ComplexityPenalty(u32),
    /// Multiplies the work to build each unit (e.g. 1.8 = 80 % longer):
    /// hand-finished parts, a low-yield process. Build time is labour, so
    /// it is also unit cost (21_ENGINES.md step 6). Engine-only.
    BuildTimePenalty(f64),
    /// Multiplies the bill of materials of each unit (e.g. 1.6 = 60 %
    /// dearer): exotic alloys, a coating only one shop can do, castings
    /// scrapped at high rates (23_DEFICIENCIES.md §3.1). Engine-only.
    MaterialCostPenalty(f64),
    /// The design's flaws hide longer: every discovery exponent the
    /// project rolls at is raised by this much, flight reviews
    /// included (23_DEFICIENCIES.md §3.2). Engines and reactors.
    HardToTest(u32),
    /// An electric thruster draws this fraction more power per newton
    /// (e.g. 0.5 = ×1.5 `power_draw_w`): panels size up with it and a
    /// stage that was fine is power-starved until it is solved
    /// (23_DEFICIENCIES.md §3.3). Electric-only.
    PowerDrawPenalty(f64),
}

impl std::fmt::Display for TechDeficiencyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TechDeficiencyKind::IspPenalty(frac) => write!(f, "-{:.0}% Isp", frac * 100.0),
            TechDeficiencyKind::MassPenalty(frac) => write!(f, "+{:.0}% mass", frac * 100.0),
            TechDeficiencyKind::ThrustPenalty(frac) => write!(f, "-{:.0}% thrust", frac * 100.0),
            TechDeficiencyKind::PowerPenalty(frac) => write!(f, "-{:.0}% power", frac * 100.0),
            TechDeficiencyKind::ComplexityPenalty(n) => write!(f, "+{} complexity", n),
            TechDeficiencyKind::BuildTimePenalty(m) => write!(f, "×{:.1} build time", m),
            TechDeficiencyKind::MaterialCostPenalty(m) => write!(f, "×{:.1} materials", m),
            TechDeficiencyKind::HardToTest(k) => write!(f, "+{} test difficulty", k),
            TechDeficiencyKind::PowerDrawPenalty(frac) => write!(f, "+{:.0}% power draw", frac * 100.0),
        }
    }
}

/// Which family of parts a technology's deficiencies apply to. Selects
/// the pool of deficiency *kinds* generated at game start: engine techs
/// draw from Isp/Mass/Thrust/Complexity, reactor techs from
/// Power/Mass/Complexity (Isp/Thrust are meaningless for a power source),
/// electric techs from PowerDraw/Thrust/Mass/Materials/HardToTest
/// (no Isp, no build time: the thruster is simple, the power is not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechDomain {
    Engine,
    Reactor,
    Electric,
}

/// A deficiency inherent to a technology, determined by the game seed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechDeficiency {
    pub id: TechDeficiencyId,
    pub description: String,
    pub kind: TechDeficiencyKind,
    /// How solvable this deficiency is (0.0 = impossible, 1.0 = easy).
    pub solvability: f64,
    /// Number of failed revision attempts across all engines.
    pub total_attempts: u32,
}

/// An experimental technology that may have deficiencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Technology {
    pub id: TechnologyId,
    pub name: String,
    pub description: String,
    /// Whether the player can use this tech in engine designs.
    pub unlocked: bool,
    /// Difficulty level: 0 = low risk, 1 = moderate, 2 = high risk.
    pub difficulty: u32,
    /// Deficiencies inherent to this tech (seed-generated).
    pub deficiencies: Vec<TechDeficiency>,
}

impl Technology {
    /// The deficiencies a company has not solved yet (22_FLAWS.md §3.6:
    /// solving is per company, so a rival adopting the tech faces the
    /// full list).
    pub fn unsolved_for<'a>(&'a self, solved: &std::collections::HashSet<TechDeficiencyId>) -> Vec<&'a TechDeficiency> {
        self.deficiencies.iter().filter(|d| !solved.contains(&d.id)).collect()
    }
}

// ── Well-known technology IDs ──

/// The experimental technologies a (cycle, propellant) family is on:
/// the propellant's, when methane or nuclear hydrogen, and the cycle's
/// once cycles carry technologies (23_DEFICIENCIES.md §4).
pub fn technologies_for(
    cycle: crate::engine::EngineCycle, preset: crate::engine_project::PropellantPreset,
) -> Vec<TechnologyId> {
    use crate::engine::EngineCycle;
    let mut out = Vec::new();
    match preset {
        crate::engine_project::PropellantPreset::Methalox => out.push(TECH_METHALOX),
        crate::engine_project::PropellantPreset::Hydrogen => out.push(TECH_NUCLEAR_THERMAL),
        _ => {}
    }
    // The harder cycle technology subsumes the easier: a full-flow
    // engine carries full-flow's deficiencies, not staged combustion's.
    match cycle {
        EngineCycle::StagedCombustion => out.push(TECH_STAGED_COMBUSTION),
        EngineCycle::FullFlow => out.push(TECH_FULL_FLOW),
        EngineCycle::ElectricPropulsion => out.push(TECH_ELECTRIC),
        _ => {}
    }
    out
}

pub const TECH_METHALOX: TechnologyId = TechnologyId(1);
pub const TECH_NUCLEAR_THERMAL: TechnologyId = TechnologyId(2);
pub const TECH_FISSION_REACTOR: TechnologyId = TechnologyId(3);
/// Oxidiser-rich preburners and the pumps that survive them: in 2000
/// nobody outside Russia had a working one (23_DEFICIENCIES.md §4.1).
pub const TECH_STAGED_COMBUSTION: TechnologyId = TechnologyId(4);
/// Full-flow staged combustion: one engine had ever run, on a stand.
pub const TECH_FULL_FLOW: TechnologyId = TechnologyId(5);
/// Electric propulsion: ion and Hall thrusters had flown, but as
/// station-keeping, not as the drive (23_DEFICIENCIES.md §4.2).
pub const TECH_ELECTRIC: TechnologyId = TechnologyId(6);

/// Generate technologies for a new game.
pub fn generate_technologies(seed: &GameSeed, cfg: &TechnologyConfig) -> Vec<Technology> {
    vec![
        generate_technology(
            seed,
            cfg,
            TECH_METHALOX,
            "Methalox",
            "Liquid methane/LOX propulsion — promising but unproven in flight",
            true,  // unlocked at start
            0,     // difficulty 0 (low risk)
            TechDomain::Engine,
        ),
        generate_technology(
            seed,
            cfg,
            TECH_NUCLEAR_THERMAL,
            "Nuclear Thermal",
            "Nuclear reactor heating hydrogen propellant — very high Isp but experimental",
            false, // unlocked by event
            2,     // difficulty 2 (high risk)
            TechDomain::Engine,
        ),
        generate_technology(
            seed,
            cfg,
            TECH_FISSION_REACTOR,
            "Fission Reactor",
            "Compact space-rated nuclear reactor producing electricity continuously — \
             enables high-power probes beyond reach of solar panels",
            false, // unlocked by event
            2,     // difficulty 2 (high risk)
            TechDomain::Reactor,
        ),
        generate_technology(
            seed,
            cfg,
            TECH_STAGED_COMBUSTION,
            "Staged Combustion",
            "Oxidiser-rich preburners driving the pumps — the highest-performance \
             cycle, and nobody outside Russia has made one work",
            true,  // unlocked at start
            1,     // difficulty 1 (moderate risk)
            TechDomain::Engine,
        ),
        generate_technology(
            seed,
            cfg,
            TECH_FULL_FLOW,
            "Full-Flow Staged Combustion",
            "Both propellants through their own preburners, gas-gas injection — \
             one engine has ever run, on a test stand",
            false, // unlocked by event
            2,     // difficulty 2 (high risk)
            TechDomain::Engine,
        ),
        generate_technology(
            seed,
            cfg,
            TECH_ELECTRIC,
            "Electric Propulsion",
            "Ion and Hall thrusters as the main drive — a few have flown for \
             station-keeping, none has pushed a spacecraft anywhere",
            true,  // unlocked at start
            1,     // difficulty 1 (moderate risk)
            TechDomain::Electric,
        ),
    ]
}

#[allow(clippy::too_many_arguments)] // one technology's identity plus the seed and the config; a builder would just rename them
fn generate_technology(
    seed: &GameSeed,
    cfg: &TechnologyConfig,
    id: TechnologyId,
    name: &str,
    description: &str,
    unlocked: bool,
    difficulty: u32,
    domain: TechDomain,
) -> Technology {
    let mut rng = seed.world_query(WorldQuery::TechDeficiencies(id));

    let deficiencies = generate_deficiencies(&mut rng, difficulty, id, domain, cfg);

    Technology {
        id,
        name: name.to_string(),
        description: description.to_string(),
        unlocked,
        difficulty,
        deficiencies,
    }
}

fn generate_deficiencies(
    rng: &mut rand::rngs::StdRng,
    difficulty: u32,
    tech_id: TechnologyId,
    domain: TechDomain,
    cfg: &TechnologyConfig,
) -> Vec<TechDeficiency> {
    // By default: difficulty 0 carries 0-2 deficiencies at solvability
    // 0.0-1.0, difficulty 1 carries 1-3 at max(-0.1..0.9, 0), difficulty
    // 2 carries 2-4 at max(-0.2..0.8, 0) — see `TechnologyConfig`.
    let count_range = cfg.deficiency_count(difficulty);
    let (min_count, max_count) = (count_range.min, count_range.max);
    let solvability_offset = -(difficulty as f64) * cfg.solvability_penalty_per_difficulty;
    let solvability_range = 1.0 - (difficulty as f64) * cfg.solvability_penalty_per_difficulty;

    let count = rng.gen_range(min_count..=max_count);
    let mut next_id = tech_id.0 * 100; // namespace deficiency IDs by tech
    let mut used_descriptions: Vec<&'static str> = Vec::new();

    (0..count).map(|_| {
        let id = TechDeficiencyId(next_id);
        next_id += 1;

        let raw_solvability = solvability_offset + rng.gen::<f64>() * solvability_range;
        let solvability = raw_solvability.max(0.0);

        // Pick deficiency kind and magnitude: the draw's floor rises a
        // little with difficulty and its ceiling a lot, and each kind
        // scales it (23_DEFICIENCIES.md §2).
        let (low, high) = cfg.magnitude_range(difficulty);
        let magnitude = rng.gen_range(low..high);

        let roll: f64 = rng.gen();
        let (kind, description) = match domain {
            TechDomain::Engine => engine_deficiency_kind(roll, magnitude, difficulty, tech_id, &mut used_descriptions, rng, cfg),
            TechDomain::Reactor => reactor_deficiency_kind(roll, magnitude, difficulty, tech_id, &mut used_descriptions, rng, cfg),
            TechDomain::Electric => electric_deficiency_kind(roll, magnitude, difficulty, tech_id, &mut used_descriptions, rng, cfg),
        };

        TechDeficiency {
            id,
            description,
            kind,
            solvability,
            total_attempts: 0,
        }
    }).collect()
}

/// Engine-domain deficiency kinds: Isp / Mass / Thrust / Build time /
/// Materials / HardToTest / Complexity.
fn engine_deficiency_kind(
    roll: f64,
    magnitude: f64,
    difficulty: u32,
    tech: TechnologyId,
    used: &mut Vec<&'static str>,
    rng: &mut rand::rngs::StdRng,
    cfg: &TechnologyConfig,
) -> (TechDeficiencyKind, String) {
    use DescKind as D;
    let (kind, desc) = if roll < 0.22 {
        (TechDeficiencyKind::IspPenalty(magnitude * cfg.isp_scale), D::Isp)
    } else if roll < 0.40 {
        (TechDeficiencyKind::MassPenalty(magnitude * cfg.mass_scale), D::Mass)
    } else if roll < 0.58 {
        (TechDeficiencyKind::ThrustPenalty(magnitude.min(cfg.thrust_cap)), D::Thrust)
    } else if roll < 0.70 {
        (TechDeficiencyKind::BuildTimePenalty(1.0 + cfg.build_time_penalty_scale * magnitude), D::BuildTime)
    } else if roll < 0.80 {
        (TechDeficiencyKind::MaterialCostPenalty(1.0 + cfg.material_cost_scale * magnitude), D::Materials)
    } else if roll < 0.90 {
        (TechDeficiencyKind::HardToTest(rng.gen_range(1..=1 + difficulty)), D::HardToTest)
    } else {
        (TechDeficiencyKind::ComplexityPenalty(rng.gen_range(1..=2 + difficulty)), D::Complexity)
    };
    (kind, pick_description(rng, used, description_pool(tech, desc)))
}

/// Reactor-domain deficiency kinds: Power / Mass / HardToTest /
/// Complexity. Isp and thrust don't apply to a power source, so the
/// primary-stat hit is a power penalty (weighted like Isp+Thrust
/// combined for engines).
fn reactor_deficiency_kind(
    roll: f64,
    magnitude: f64,
    difficulty: u32,
    tech: TechnologyId,
    used: &mut Vec<&'static str>,
    rng: &mut rand::rngs::StdRng,
    cfg: &TechnologyConfig,
) -> (TechDeficiencyKind, String) {
    use DescKind as D;
    let (kind, desc) = if roll < 0.45 {
        (TechDeficiencyKind::PowerPenalty(magnitude * cfg.power_scale), D::Power)
    } else if roll < 0.70 {
        (TechDeficiencyKind::MassPenalty(magnitude * cfg.mass_scale), D::Mass)
    } else if roll < 0.82 {
        (TechDeficiencyKind::HardToTest(rng.gen_range(1..=1 + difficulty)), D::HardToTest)
    } else {
        (TechDeficiencyKind::ComplexityPenalty(rng.gen_range(1..=2 + difficulty)), D::Complexity)
    };
    (kind, pick_description(rng, used, description_pool(tech, desc)))
}

/// Electric-domain deficiency kinds: PowerDraw / Thrust / Mass /
/// Materials / HardToTest / Complexity. Isp is what the family is for
/// and the thruster itself is cheap to build, so neither is a kind here;
/// the primary-stat hit is power per newton (23_DEFICIENCIES.md §4.2).
fn electric_deficiency_kind(
    roll: f64,
    magnitude: f64,
    difficulty: u32,
    tech: TechnologyId,
    used: &mut Vec<&'static str>,
    rng: &mut rand::rngs::StdRng,
    cfg: &TechnologyConfig,
) -> (TechDeficiencyKind, String) {
    use DescKind as D;
    let (kind, desc) = if roll < 0.40 {
        (TechDeficiencyKind::PowerDrawPenalty(magnitude * cfg.power_draw_scale), D::PowerDraw)
    } else if roll < 0.55 {
        (TechDeficiencyKind::ThrustPenalty(magnitude.min(cfg.thrust_cap)), D::Thrust)
    } else if roll < 0.70 {
        (TechDeficiencyKind::MassPenalty(magnitude * cfg.mass_scale), D::Mass)
    } else if roll < 0.80 {
        (TechDeficiencyKind::MaterialCostPenalty(1.0 + cfg.material_cost_scale * magnitude), D::Materials)
    } else if roll < 0.90 {
        (TechDeficiencyKind::HardToTest(rng.gen_range(1..=1 + difficulty)), D::HardToTest)
    } else {
        (TechDeficiencyKind::ComplexityPenalty(rng.gen_range(1..=2 + difficulty)), D::Complexity)
    };
    (kind, pick_description(rng, used, description_pool(tech, desc)))
}

/// The axis a description pool is written for: one per
/// `TechDeficiencyKind`, without the magnitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescKind {
    Isp,
    Mass,
    Thrust,
    BuildTime,
    Materials,
    HardToTest,
    Complexity,
    Power,
    PowerDraw,
}

/// The stock descriptions for a kind on a technology: written for the
/// family where it is known (methane, hydrogen through a core,
/// oxidiser-rich preburners, full flow, a reactor, an ion drive) so a
/// player reads why *this* technology falls short, not a generic
/// "immature process" (23_DEFICIENCIES.md §3.3, step 7). A technology
/// never shows the same line twice (`pick_description`), so every pool
/// holds at least four lines — one more than the largest count.
fn description_pool(tech: TechnologyId, kind: DescKind) -> &'static [&'static str] {
    use DescKind as D;
    match (tech, kind) {
        // ── Methalox ──
        (TECH_METHALOX, D::Isp) => &[
            "Methane burns cooler than modelled at the design mixture",
            "Incomplete combustion leaves unburnt methane in the plume",
            "Kerosene-style injector wastes methane's mixing advantage",
            "Methane film cooling bleeds more Isp than the books say",
        ],
        (TECH_METHALOX, D::Mass) => &[
            "Methane's low density needs bigger tanks and plumbing",
            "Subcooling hardware to keep methane dense adds mass",
            "Chamber walls thickened against methane's coking surprises",
            "Common-bulkhead insulation heavier than planned",
        ],
        (TECH_METHALOX, D::Thrust) => &[
            "Combustion instability caps chamber pressure on methane",
            "Methane turbopump cavitates above a modest flow",
            "Injector throttled to keep the methane flame anchored",
            "Chamber pressure held down until the acoustics are known",
        ],
        (TECH_METHALOX, D::BuildTime) => &[
            "Every methane injector is tuned by hand on the stand",
            "Methane-compatible seals sourced and fitted one at a time",
            "Long acceptance firings to catch methane coking",
            "Chamber coatings reworked after each first hot fire",
        ],
        (TECH_METHALOX, D::Materials) => &[
            "Methane-rated valves come from a single supplier",
            "Copper-alloy liners for methane cast at low yield",
            "Subcooled methane sensors imported at a premium",
            "Igniter hardware for methane built to order",
        ],
        (TECH_METHALOX, D::HardToTest) => &[
            "No methane test stand runs at flight chamber pressure",
            "Methane combustion diagnostics still being developed",
            "Methane's start transient differs between stand and flight",
            "Coking only shows after more firings than the stand allows",
        ],
        (TECH_METHALOX, D::Complexity) => &[
            "Methane ignition sequencing poorly understood",
            "Methane coking behaviour not yet characterised",
            "Two cryogens at different temperatures complicate loading",
            "Methane leak detection immature",
        ],
        // ── Nuclear thermal ──
        (TECH_NUCLEAR_THERMAL, D::Isp) => &[
            "Core outlet temperature held down to spare the fuel",
            "Hydrogen dissociation lower than the reference assumed",
            "Flow channelling leaves part of the hydrogen cool",
            "Nozzle throat erosion from hot hydrogen costs expansion",
        ],
        (TECH_NUCLEAR_THERMAL, D::Mass) => &[
            "Shadow shield heavier than the reference design",
            "Reflector and pressure vessel thickened for margin",
            "Hydrogen pump sized up for the core's pressure drop",
            "Control drums and actuators heavier than modelled",
        ],
        (TECH_NUCLEAR_THERMAL, D::Thrust) => &[
            "Core power capped by fuel-element cracking",
            "Hydrogen flow limited by corrosion of the fuel coating",
            "Reactor startup too slow for full thrust on demand",
            "Hot-spot factors force a derated core",
        ],
        (TECH_NUCLEAR_THERMAL, D::BuildTime) => &[
            "Fuel elements fabricated and inspected one at a time",
            "Core assembly under nuclear safety procedures",
            "Every element coated and tested before stacking",
            "Hydrogen-side hardware qualified to reactor standards",
        ],
        (TECH_NUCLEAR_THERMAL, D::Materials) => &[
            "Enriched fuel priced and allocated by a single agency",
            "Carbide fuel coatings from one laboratory",
            "Beryllium reflector stock in short supply",
            "Radiation-hard sensors at specialist prices",
        ],
        (TECH_NUCLEAR_THERMAL, D::HardToTest) => &[
            "No facility can fire the core with hydrogen at full power",
            "Exhaust scrubbing limits every hot test to minutes",
            "Fuel-element behaviour only seen at operating temperature",
            "Instrumentation fails in the neutron flux",
        ],
        (TECH_NUCLEAR_THERMAL, D::Complexity) => &[
            "Reactor control and engine control not yet unified",
            "Hydrogen embrittlement of the core poorly understood",
            "Post-shutdown cooling needs propellant and sequencing",
            "Fuel-element failure modes still being catalogued",
        ],
        // ── Fission reactor ──
        (TECH_FISSION_REACTOR, D::Power) => &[
            "Neutron flux lower than predicted",
            "Fuel element power density shortfall",
            "Control-drum calibration limits output",
            "Thermoelectric conversion short of the sample cells",
        ],
        (TECH_FISSION_REACTOR, D::Mass) => &[
            "Heavier radiation shielding required",
            "Oversized radiator for thermal margin",
            "Reinforced pressure vessel needed",
            "Heat-pipe network heavier than the single-loop model",
        ],
        (TECH_FISSION_REACTOR, D::HardToTest) => &[
            "No test facility can run the core at power",
            "Neutron instrumentation saturates at rated flux",
            "Transient behaviour only seen at full power",
            "Radiator performance cannot be measured outside vacuum",
        ],
        (TECH_FISSION_REACTOR, D::Complexity) => &[
            "Immature fuel fabrication processes",
            "Poorly understood reactor kinetics",
            "Materials compatibility under irradiation",
            "Launch-safety analysis redone for every configuration",
        ],
        // ── Staged combustion ──
        (TECH_STAGED_COMBUSTION, D::Isp) => &[
            "Preburner run cool to spare the turbine, costing pressure",
            "Oxidiser-rich gas enters the chamber poorly mixed",
            "Injector tuned for safety rather than mixing efficiency",
            "Turbine pressure drop eats into chamber pressure",
        ],
        (TECH_STAGED_COMBUSTION, D::Mass) => &[
            "Preburner and hot-gas ducts thick against oxygen fire",
            "Turbopump housing overbuilt for the preburner pressure",
            "Heavy hot-gas manifold to survive oxidiser-rich flow",
            "Extra valving to sequence the preburner safely",
        ],
        (TECH_STAGED_COMBUSTION, D::Thrust) => &[
            "Turbine inlet temperature capped to stop metal fires",
            "Preburner instability limits pump power",
            "Oxidiser pump seals leak above a modest shaft speed",
            "Chamber pressure derated until the hot-gas path proves out",
        ],
        (TECH_STAGED_COMBUSTION, D::BuildTime) => &[
            "Oxygen-compatible coatings applied and checked by hand",
            "Preburner injectors balanced one element at a time",
            "Every turbopump torn down after its acceptance firing",
            "Hot-gas ducting welded and X-rayed in sections",
        ],
        (TECH_STAGED_COMBUSTION, D::Materials) => &[
            "Oxygen-resistant superalloys from a single mill",
            "Preburner liner coatings only one shop can apply",
            "Turbine castings scrapped at high rates",
            "Oxidiser-rich valve seats imported at specialist prices",
        ],
        (TECH_STAGED_COMBUSTION, D::HardToTest) => &[
            "Preburner metal fires leave nothing to diagnose",
            "Hot-gas instrumentation fails in the oxidiser-rich stream",
            "Turbopump wear only shows after many starts",
            "No component stand reproduces the hot-gas path",
        ],
        (TECH_STAGED_COMBUSTION, D::Complexity) => &[
            "Oxygen fire in the hot-gas path poorly understood",
            "Start sequence for the preburner immature",
            "Turbopump bearing life in oxidiser-rich gas unknown",
            "Preburner-chamber coupling not yet modelled",
        ],
        // ── Full-flow staged combustion ──
        (TECH_FULL_FLOW, D::Isp) => &[
            "Both preburners run cool to spare their turbines",
            "Gas-gas injector mixing short of the ideal",
            "Fuel-rich and oxidiser-rich gases arrive out of balance",
            "Chamber pressure short while the preburners are tuned",
        ],
        (TECH_FULL_FLOW, D::Mass) => &[
            "Two preburners and two hot-gas paths, both built heavy",
            "Interpropellant seal and purge hardware adds mass",
            "Both turbopump housings overbuilt against their preburners",
            "Twin hot-gas manifolds thick-walled for margin",
        ],
        (TECH_FULL_FLOW, D::Thrust) => &[
            "Oxidiser preburner temperature capped against metal fire",
            "Fuel preburner soots and limits turbine power",
            "Balancing two turbopumps limits the throttle range",
            "Gas-gas injector instability caps chamber pressure",
        ],
        (TECH_FULL_FLOW, D::BuildTime) => &[
            "Two preburners, each balanced by hand",
            "Gas-gas injector elements individually flow-checked",
            "Both turbopumps torn down after every acceptance firing",
            "Hot-gas ducting for two streams welded in sections",
        ],
        (TECH_FULL_FLOW, D::Materials) => &[
            "Two preburners, two exotic alloys, each from one mill",
            "Gas-gas injector faces from a single supplier",
            "Turbine castings for two pumps scrapped at high rates",
            "Preburner liners coated by one shop",
        ],
        (TECH_FULL_FLOW, D::HardToTest) => &[
            "No stand reproduces two hot-gas paths at once",
            "Preburner fires leave nothing to diagnose",
            "Gas-gas combustion diagnostics still being developed",
            "Twin-turbopump interactions only show at full power",
        ],
        (TECH_FULL_FLOW, D::Complexity) => &[
            "Start sequence for two preburners immature",
            "Gas-gas injection dynamics barely understood",
            "Oxygen fire in the hot-gas path poorly understood",
            "Twin-turbopump transients not yet modelled",
        ],
        // ── Electric propulsion ──
        (TECH_ELECTRIC, D::PowerDraw) => &[
            "Power processing unit losses far above design",
            "Beam divergence wastes accelerated propellant",
            "Cathode heater draws power at all throttle settings",
            "Discharge losses higher than the laboratory thruster",
        ],
        (TECH_ELECTRIC, D::Thrust) => &[
            "Grid erosion limits beam current",
            "Discharge instabilities cap the throttle",
            "Propellant feed cannot hold flow at full power",
            "Thermal limits on the discharge chamber cap power",
        ],
        (TECH_ELECTRIC, D::Mass) => &[
            "Power processing unit heavier than planned",
            "Oversized radiators for the thruster's waste heat",
            "Magnetic circuit needs more iron than modelled",
            "Gimbal and high-voltage harness heavier than planned",
        ],
        (TECH_ELECTRIC, D::Materials) => &[
            "Grids machined from a single supplier's molybdenum",
            "Hollow cathode inserts from one laboratory",
            "High-voltage components screened at low yield",
            "Xenon flow controllers built to order",
        ],
        (TECH_ELECTRIC, D::HardToTest) => &[
            "Vacuum chamber backpressure masks true performance",
            "Thousand-hour wear-out only shows in flight",
            "Facility effects on the plume not understood",
            "Spacecraft charging only seen in orbit",
        ],
        (TECH_ELECTRIC, D::Complexity) => &[
            "High-voltage isolation immature",
            "Thruster-spacecraft interactions poorly understood",
            "Propellant management at micro-flow rates",
            "Plume sputtering of nearby surfaces not characterised",
        ],
        // ── Any other technology: the generic engine lines ──
        (_, D::Isp) => &[
            "Incomplete combustion characteristics",
            "Non-optimal mixture ratio range",
            "Nozzle erosion from exhaust products",
            "Injector mixing below the design efficiency",
        ],
        (_, D::Mass) => &[
            "Heavier propellant handling systems required",
            "Additional thermal management mass",
            "Reinforced injector design needed",
            "Structural margins raised for the unknowns",
        ],
        (_, D::Thrust) => &[
            "Chamber pressure limitations",
            "Propellant feed instabilities",
            "Reduced injector throughput",
            "Combustion instability caps the throttle",
        ],
        (_, D::BuildTime) => &[
            "Hand-finished combustion chamber",
            "Low-yield manufacturing process",
            "Lengthy per-unit acceptance testing",
            "Rework after every first hot fire",
        ],
        (_, D::Materials) => &[
            "Exotic alloys from a single mill",
            "Coatings only one shop can apply",
            "Precision castings scrapped at high rates",
            "Specialist components built to order",
        ],
        (_, D::HardToTest) => &[
            "Instrumentation immature for the combustion regime",
            "Test stand cannot reproduce flight conditions",
            "Failure signatures not yet understood",
            "Wear only shows after more firings than the stand allows",
        ],
        (_, D::Complexity) => &[
            "Immature manufacturing processes",
            "Poorly understood failure modes",
            "Materials compatibility issues",
            "Start and shutdown sequencing immature",
        ],
        (_, D::Power) => &[
            "Output lower than predicted",
            "Conversion efficiency shortfall",
            "Thermal limits cap the output",
            "Control margins derate the source",
        ],
        (_, D::PowerDraw) => &[
            "Power processing losses above design",
            "Thruster efficiency below the laboratory article",
            "Auxiliary loads draw at all throttle settings",
            "Beam losses waste input power",
        ],
    }
}

/// A line from the pool the technology has not used yet; the whole
/// pool again only if every line is taken.
fn pick_description(
    rng: &mut rand::rngs::StdRng,
    used: &mut Vec<&'static str>,
    pool: &'static [&'static str],
) -> String {
    let fresh: Vec<&'static str> = pool.iter().copied().filter(|d| !used.contains(d)).collect();
    let options: &[&'static str] = if fresh.is_empty() { pool } else { &fresh };
    let chosen = options[rng.gen_range(0..options.len())];
    used.push(chosen);
    chosen.to_string()
}

/// Attempt to solve a tech deficiency during revision. Returns true if
/// solved; the caller records it on the company. `already_solved_elsewhere`
/// is whether another of the company's designs has solved it, which
/// makes it much easier to solve again.
pub fn attempt_solve(
    deficiency: &mut TechDeficiency,
    already_solved_elsewhere: bool,
    rng: &mut rand::rngs::StdRng,
    cfg: &TechnologyConfig,
) -> bool {
    deficiency.total_attempts += 1;

    let chance = if already_solved_elsewhere {
        (deficiency.solvability * cfg.solved_elsewhere_multiplier).min(cfg.solve_chance_cap)
    } else {
        deficiency.solvability
    };

    rng.gen::<f64>() < chance
}

/// Message about failed attempts (same regardless of solvability).
pub fn failure_hint(attempts: u32) -> Option<&'static str> {
    match attempts {
        3 => Some("Engineers have made multiple attempts without success"),
        5 => Some("Significant engineering effort invested with no breakthrough"),
        7 => Some("Extensive revision attempts have not resolved this issue"),
        10 => Some("Engineers recommend considering alternative approaches"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_methalox() {
        let seed = GameSeed::new(42);
        let techs = generate_technologies(&seed, &TechnologyConfig::default());
        let methalox = techs.iter().find(|t| t.id == TECH_METHALOX).unwrap();
        assert!(methalox.unlocked);
        assert_eq!(methalox.difficulty, 0);
        assert!(methalox.deficiencies.len() <= 2);
    }

    #[test]
    fn test_generate_nerva() {
        let seed = GameSeed::new(42);
        let techs = generate_technologies(&seed, &TechnologyConfig::default());
        let nerva = techs.iter().find(|t| t.id == TECH_NUCLEAR_THERMAL).unwrap();
        assert!(!nerva.unlocked);
        assert_eq!(nerva.difficulty, 2);
        assert!(nerva.deficiencies.len() >= 2);
    }

    #[test]
    fn test_fission_reactor_uses_reactor_domain_kinds() {
        // The reactor tech must never surface Isp/Thrust penalties —
        // only Power/Mass/Complexity. Sweep many seeds so we exercise
        // the full kind distribution.
        let mut saw_power = false;
        for s in 0..300 {
            let seed = GameSeed::new(s);
            let techs = generate_technologies(&seed, &TechnologyConfig::default());
            let reactor = techs.iter().find(|t| t.id == TECH_FISSION_REACTOR).unwrap();
            for def in &reactor.deficiencies {
                match def.kind {
                    TechDeficiencyKind::PowerPenalty(_) => saw_power = true,
                    TechDeficiencyKind::MassPenalty(_)
                    | TechDeficiencyKind::ComplexityPenalty(_)
                    | TechDeficiencyKind::HardToTest(_) => {}
                    TechDeficiencyKind::IspPenalty(_)
                    | TechDeficiencyKind::ThrustPenalty(_)
                    | TechDeficiencyKind::BuildTimePenalty(_)
                    | TechDeficiencyKind::MaterialCostPenalty(_)
                    | TechDeficiencyKind::PowerDrawPenalty(_) => {
                        panic!("reactor tech produced an engine-only deficiency kind: {:?}", def.kind);
                    }
                }
            }
        }
        assert!(saw_power, "reactor tech should produce PowerPenalty deficiencies across seeds");
    }

    /// Engine technologies can roll a build-time penalty, and its
    /// multiplier stays in the configured band (×1.2 to ×2.4 at the
    /// default magnitudes and scale).
    #[test]
    fn engine_techs_roll_build_time_penalties_in_range() {
        let mut seen = 0;
        for s in 0..300 {
            let seed = GameSeed::new(s);
            let techs = generate_technologies(&seed, &TechnologyConfig::default());
            for tech in techs.iter().filter(|t| t.id != TECH_FISSION_REACTOR) {
                for def in &tech.deficiencies {
                    if let TechDeficiencyKind::BuildTimePenalty(m) = def.kind {
                        seen += 1;
                        // d0 1.4–3.0, d2 1.8–5.0 (23_DEFICIENCIES.md §2).
                        assert!((1.4..=5.0 + 1e-9).contains(&m), "multiplier {m}");
                    }
                }
            }
        }
        assert!(seen > 20, "only {seen} build-time penalties in 300 worlds");
    }

    #[test]
    fn test_engine_techs_never_use_power_penalty() {
        // Engine-domain techs must never surface a PowerPenalty.
        for s in 0..300 {
            let seed = GameSeed::new(s);
            let techs = generate_technologies(&seed, &TechnologyConfig::default());
            for tech in techs.iter().filter(|t| t.id != TECH_FISSION_REACTOR) {
                for def in &tech.deficiencies {
                    assert!(
                        !matches!(def.kind, TechDeficiencyKind::PowerPenalty(_)),
                        "engine tech {:?} produced a PowerPenalty", tech.name,
                    );
                }
            }
        }
    }

    #[test]
    fn test_solvability_clamped_to_zero() {
        for s in 0..100 {
            let seed = GameSeed::new(s);
            let techs = generate_technologies(&seed, &TechnologyConfig::default());
            for tech in &techs {
                for def in &tech.deficiencies {
                    assert!(def.solvability >= 0.0,
                        "Solvability should be >= 0, got {} in {:?}", def.solvability, tech.name);
                }
            }
        }
    }

    #[test]
    fn test_difficulty_2_has_impossible_sometimes() {
        let mut found_impossible = false;
        for s in 0..200 {
            let seed = GameSeed::new(s);
            let techs = generate_technologies(&seed, &TechnologyConfig::default());
            let nerva = techs.iter().find(|t| t.id == TECH_NUCLEAR_THERMAL).unwrap();
            if nerva.deficiencies.iter().any(|d| d.solvability == 0.0) {
                found_impossible = true;
                break;
            }
        }
        assert!(found_impossible, "Difficulty 2 should sometimes produce impossible deficiencies");
    }

    #[test]
    fn test_deterministic() {
        let seed = GameSeed::new(99);
        let t1 = generate_technologies(&seed, &TechnologyConfig::default());
        let t2 = generate_technologies(&seed, &TechnologyConfig::default());
        assert_eq!(t1.len(), t2.len());
        for (a, b) in t1.iter().zip(t2.iter()) {
            assert_eq!(a.deficiencies.len(), b.deficiencies.len());
            for (da, db) in a.deficiencies.iter().zip(b.deficiencies.iter()) {
                assert_eq!(da.solvability, db.solvability);
            }
        }
    }

    #[test]
    fn test_attempt_solve_unsolved() {
        use rand::SeedableRng;
        let seed = GameSeed::new(42);
        let techs = generate_technologies(&seed, &TechnologyConfig::default());
        // Find a deficiency with decent solvability
        let tech = &techs[0]; // methalox
        if let Some(def) = tech.deficiencies.iter().find(|d| d.solvability > 0.5) {
            let mut def = def.clone();
            let mut rng = rand::rngs::StdRng::seed_from_u64(1);
            // Try many times — should eventually solve
            let mut solved = false;
            for _ in 0..20 {
                if attempt_solve(&mut def, false, &mut rng, &TechnologyConfig::default()) {
                    solved = true;
                    break;
                }
            }
            assert!(solved, "Should eventually solve a high-solvability deficiency");
        }
    }

    #[test]
    fn test_solved_elsewhere_boosts_chance() {
        use rand::SeedableRng;
        let def = TechDeficiency {
            id: TechDeficiencyId(1),
            description: "test".into(),
            kind: TechDeficiencyKind::IspPenalty(0.1),
            solvability: 0.3,
            total_attempts: 0,
        };

        let mut successes_without = 0;
        let mut successes_with = 0;
        for s in 0..1000 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(s);
            let mut d1 = def.clone();
            let mut d2 = def.clone();
            if attempt_solve(&mut d1, false, &mut rng, &TechnologyConfig::default()) { successes_without += 1; }
            let mut rng2 = rand::rngs::StdRng::seed_from_u64(s);
            if attempt_solve(&mut d2, true, &mut rng2, &TechnologyConfig::default()) { successes_with += 1; }
        }
        assert!(successes_with > successes_without * 2,
            "Solved elsewhere should greatly boost success: {} vs {}",
            successes_with, successes_without);
    }

    /// 23_DEFICIENCIES.md §2: each kind lands in its own band, and the
    /// bands widen with difficulty so the top of difficulty 2 is an
    /// engine that is essentially unusable until revised.
    #[test]
    fn magnitudes_land_in_their_kinds_bands() {
        let cfg = TechnologyConfig::default();
        assert_eq!(cfg.magnitude_range(0), (0.10, 0.50));
        assert_eq!(cfg.magnitude_range(2), (0.20, 1.0));
        let mut top = [0.0f64; 4]; // isp, thrust, mass, build time
        for s in 0..400 {
            let techs = generate_technologies(&GameSeed::new(s), &cfg);
            for tech in &techs {
                let (lo, hi) = cfg.magnitude_range(tech.difficulty);
                for def in &tech.deficiencies {
                    match def.kind {
                        TechDeficiencyKind::IspPenalty(f) => {
                            assert!(f >= lo * cfg.isp_scale - 1e-9 && f <= hi * cfg.isp_scale + 1e-9, "isp {f}");
                            top[0] = top[0].max(f);
                        }
                        TechDeficiencyKind::ThrustPenalty(f) => {
                            assert!(f >= lo - 1e-9 && f <= cfg.thrust_cap + 1e-9, "thrust {f}");
                            top[1] = top[1].max(f);
                        }
                        TechDeficiencyKind::MassPenalty(f) => {
                            assert!(f >= lo * cfg.mass_scale - 1e-9 && f <= hi * cfg.mass_scale + 1e-9, "mass {f}");
                            top[2] = top[2].max(f);
                        }
                        TechDeficiencyKind::PowerPenalty(f) => {
                            assert!(f >= lo * cfg.power_scale - 1e-9 && f <= hi * cfg.power_scale + 1e-9, "power {f}");
                        }
                        TechDeficiencyKind::BuildTimePenalty(m) => top[3] = top[3].max(m),
                        TechDeficiencyKind::MaterialCostPenalty(m) => {
                            assert!(m >= 1.0 + lo * cfg.material_cost_scale - 1e-9
                                && m <= 1.0 + hi * cfg.material_cost_scale + 1e-9, "materials {m}");
                        }
                        TechDeficiencyKind::ComplexityPenalty(_) => {}
                        TechDeficiencyKind::HardToTest(k) => assert!((1..=1 + tech.difficulty).contains(&k), "hard to test {k}"),
                        TechDeficiencyKind::PowerDrawPenalty(f) => {
                            assert!(f >= lo * cfg.power_draw_scale - 1e-9 && f <= hi * cfg.power_draw_scale + 1e-9, "power draw {f}");
                        }
                    }
                }
            }
        }
        // The worst of 400 worlds reaches the brutal end of difficulty 2.
        assert!(top[0] > 0.35, "worst Isp loss {:.2}", top[0]);
        assert!(top[1] > 0.75, "worst thrust loss {:.2}", top[1]);
        assert!(top[2] > 1.3, "worst mass gain {:.2}", top[2]);
        assert!(top[3] > 4.5, "worst build time {:.2}", top[3]);
    }

    /// 23_DEFICIENCIES.md step 7: every kind a technology can roll has
    /// a pool written for it with more lines than the largest count, so
    /// a technology never shows the same line twice.
    #[test]
    fn descriptions_are_per_family_and_never_repeat_on_a_technology() {
        use DescKind as D;
        let cfg = TechnologyConfig::default();
        let all = [D::Isp, D::Mass, D::Thrust, D::BuildTime, D::Materials, D::HardToTest, D::Complexity, D::Power, D::PowerDraw];
        let techs = [TECH_METHALOX, TECH_NUCLEAR_THERMAL, TECH_FISSION_REACTOR, TECH_STAGED_COMBUSTION, TECH_FULL_FLOW, TECH_ELECTRIC, TechnologyId(999)];
        let max_count = (0..3).map(|d| cfg.deficiency_count(d).max).max().unwrap() as usize;
        for tech in techs {
            for kind in all {
                let pool = description_pool(tech, kind);
                assert!(pool.len() >= max_count, "{tech:?} {kind:?} has {} lines for up to {max_count} deficiencies", pool.len());
                let mut sorted = pool.to_vec();
                sorted.sort_unstable();
                sorted.dedup();
                assert_eq!(sorted.len(), pool.len(), "{tech:?} {kind:?} repeats a line");
            }
        }
        // Family flavour: the generic fallback is not what the known
        // technologies get.
        assert_ne!(description_pool(TECH_METHALOX, D::Isp), description_pool(TechnologyId(999), D::Isp));
        assert_ne!(description_pool(TECH_ELECTRIC, D::Thrust), description_pool(TECH_STAGED_COMBUSTION, D::Thrust));
        for s in 0..300 {
            for tech in generate_technologies(&GameSeed::new(s), &cfg) {
                let mut seen: Vec<&str> = tech.deficiencies.iter().map(|d| d.description.as_str()).collect();
                let n = seen.len();
                seen.sort_unstable();
                seen.dedup();
                assert_eq!(seen.len(), n, "seed {s}: {} repeats a description", tech.name);
            }
        }
    }

    /// 23_DEFICIENCIES.md §4.2: electric propulsion is an unlocked
    /// difficulty-1 technology, the only home of `PowerDrawPenalty`,
    /// and every electric design is on it.
    #[test]
    fn electric_technology_owns_the_power_draw_kind() {
        use crate::engine::EngineCycle;
        use crate::engine_project::PropellantPreset;
        let cfg = TechnologyConfig::default();
        let mut saw_power_draw = false;
        for s in 0..300 {
            let techs = generate_technologies(&GameSeed::new(s), &cfg);
            let electric = techs.iter().find(|t| t.id == TECH_ELECTRIC).unwrap();
            assert!(electric.unlocked);
            assert_eq!(electric.difficulty, 1);
            for tech in &techs {
                for def in &tech.deficiencies {
                    let power_draw = matches!(def.kind, TechDeficiencyKind::PowerDrawPenalty(_));
                    assert!(!power_draw || tech.id == TECH_ELECTRIC, "{} drew a PowerDrawPenalty", tech.name);
                    saw_power_draw |= power_draw;
                    if tech.id == TECH_ELECTRIC {
                        assert!(!matches!(def.kind,
                            TechDeficiencyKind::IspPenalty(_) | TechDeficiencyKind::BuildTimePenalty(_)
                            | TechDeficiencyKind::PowerPenalty(_)), "electric drew {}", def.kind);
                    }
                }
            }
        }
        assert!(saw_power_draw, "no PowerDrawPenalty in 300 worlds");
        assert_eq!(technologies_for(EngineCycle::ElectricPropulsion, PropellantPreset::Xenon), vec![TECH_ELECTRIC]);
        assert_eq!(technologies_for(EngineCycle::GasGenerator, PropellantPreset::Kerolox), Vec::<TechnologyId>::new());
    }

    /// 23_DEFICIENCIES.md §4.1: staged combustion is a technology from
    /// the start, full-flow a locked harder one, and a design carries
    /// the harder cycle's technology rather than both cycle techs.
    #[test]
    fn cycle_technologies_exist_and_are_carried_by_their_cycles() {
        use crate::engine::EngineCycle;
        use crate::engine_project::PropellantPreset;
        let techs = generate_technologies(&GameSeed::new(42), &TechnologyConfig::default());
        let sc = techs.iter().find(|t| t.id == TECH_STAGED_COMBUSTION).unwrap();
        assert!(sc.unlocked && sc.difficulty == 1 && (1..=3).contains(&sc.deficiencies.len()));
        let ff = techs.iter().find(|t| t.id == TECH_FULL_FLOW).unwrap();
        assert!(!ff.unlocked && ff.difficulty == 2 && ff.deficiencies.len() >= 2);
        for t in [sc, ff] {
            assert!(t.deficiencies.iter().all(|d| !matches!(d.kind, TechDeficiencyKind::PowerPenalty(_))));
        }
        assert_eq!(technologies_for(EngineCycle::GasGenerator, PropellantPreset::Kerolox), vec![]);
        assert_eq!(technologies_for(EngineCycle::StagedCombustion, PropellantPreset::Kerolox), vec![TECH_STAGED_COMBUSTION]);
        assert_eq!(technologies_for(EngineCycle::StagedCombustion, PropellantPreset::Methalox), vec![TECH_METHALOX, TECH_STAGED_COMBUSTION]);
        assert_eq!(technologies_for(EngineCycle::FullFlow, PropellantPreset::Hydrolox), vec![TECH_FULL_FLOW]);
    }
}
