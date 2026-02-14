/// Cost constants for rocket budget system
pub mod costs {
    /// Standard gravity at Earth's surface in m/s²
    pub const G0: f64 = 9.81;

    /// Starting budget in dollars
    pub const STARTING_BUDGET: f64 = 500_000_000.0;

    /// Cost per engine test in dollars
    pub const ENGINE_TEST_COST: f64 = 1_000_000.0;

    /// Cost per rocket test in dollars
    pub const ROCKET_TEST_COST: f64 = 2_000_000.0;

    /// Cost to fix a discovered flaw in dollars
    pub const FLAW_FIX_COST: f64 = 5_000_000.0;

    /// Propellant densities in kg/m³
    /// These are effective combined densities accounting for mixture ratios
    pub const KEROLOX_DENSITY_KG_M3: f64 = 1020.0;
    pub const HYDROLOX_DENSITY_KG_M3: f64 = 290.0;

    /// Tank structural mass as a fraction of propellant mass
    pub const KEROLOX_TANK_MASS_RATIO: f64 = 0.06;
    pub const HYDROLOX_TANK_MASS_RATIO: f64 = 0.10;

    /// Solid motor fixed mass ratio (propellant mass / total mass)
    pub const SOLID_MASS_RATIO: f64 = 0.88;

    /// Solid motor propellant density in kg/m³
    pub const SOLID_DENSITY_KG_M3: f64 = 1800.0;

    /// Solid motor "tank" mass ratio (casing mass as fraction of propellant)
    pub const SOLID_TANK_MASS_RATIO: f64 = 0.136;

    /// Methalox (CH4/LOX) effective combined density in kg/m³
    pub const METHALOX_DENSITY_KG_M3: f64 = 830.0;

    /// Methalox tank structural mass as a fraction of propellant mass
    pub const METHALOX_TANK_MASS_RATIO: f64 = 0.07;

    /// Hypergolic (NTO/UDMH) effective combined density in kg/m³
    pub const HYPERGOLIC_DENSITY_KG_M3: f64 = 1200.0;

    /// Hypergolic tank structural mass as a fraction of propellant mass
    pub const HYPERGOLIC_TANK_MASS_RATIO: f64 = 0.05;

    /// Structural mass for booster attachment points in kg
    pub const BOOSTER_ATTACHMENT_MASS_KG: f64 = 500.0;

    /// Cost for booster attachment hardware in dollars
    pub const BOOSTER_ATTACHMENT_COST: f64 = 1_000_000.0;
}
