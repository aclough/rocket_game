/// Propellant types used by rocket engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Propellant {
    LOX,
    RP1,
    LH2,
    Methane,
    UDMH,
    NTO,
    SolidMix,
}

impl Propellant {
    /// Density in kg/L
    pub fn density_kg_per_l(&self) -> f64 {
        match self {
            Propellant::LOX => 1.141,
            Propellant::RP1 => 0.82,
            Propellant::LH2 => 0.071,
            Propellant::Methane => 0.422,
            Propellant::UDMH => 0.791,
            Propellant::NTO => 1.45,
            Propellant::SolidMix => 1.77,
        }
    }

    /// Whether this propellant requires cryogenic storage
    pub fn is_cryogenic(&self) -> bool {
        matches!(self, Propellant::LOX | Propellant::LH2 | Propellant::Methane)
    }

    /// Cost per kilogram in dollars
    pub fn cost_per_kg(&self) -> f64 {
        match self {
            Propellant::LOX => 0.16,
            Propellant::RP1 => 1.10,
            Propellant::LH2 => 3.00,
            Propellant::Methane => 0.80,
            Propellant::UDMH => 30.00,
            Propellant::NTO => 15.00,
            Propellant::SolidMix => 12.00,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Propellant::LOX => "Liquid Oxygen",
            Propellant::RP1 => "RP-1 (Kerosene)",
            Propellant::LH2 => "Liquid Hydrogen",
            Propellant::Methane => "Liquid Methane",
            Propellant::UDMH => "UDMH",
            Propellant::NTO => "Nitrogen Tetroxide",
            Propellant::SolidMix => "Solid Propellant Mix",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_densities_positive() {
        for p in [
            Propellant::LOX, Propellant::RP1, Propellant::LH2,
            Propellant::Methane, Propellant::UDMH, Propellant::NTO,
            Propellant::SolidMix,
        ] {
            assert!(p.density_kg_per_l() > 0.0, "{:?} density should be positive", p);
        }
    }

    #[test]
    fn test_cryogenic() {
        assert!(Propellant::LOX.is_cryogenic());
        assert!(Propellant::LH2.is_cryogenic());
        assert!(Propellant::Methane.is_cryogenic());
        assert!(!Propellant::RP1.is_cryogenic());
        assert!(!Propellant::UDMH.is_cryogenic());
        assert!(!Propellant::NTO.is_cryogenic());
        assert!(!Propellant::SolidMix.is_cryogenic());
    }

    #[test]
    fn test_costs_positive() {
        for p in [
            Propellant::LOX, Propellant::RP1, Propellant::LH2,
            Propellant::Methane, Propellant::UDMH, Propellant::NTO,
            Propellant::SolidMix,
        ] {
            assert!(p.cost_per_kg() > 0.0, "{:?} cost should be positive", p);
        }
    }

    #[test]
    fn test_lh2_lowest_density() {
        // LH2 is famously the least dense rocket propellant
        for p in [
            Propellant::LOX, Propellant::RP1, Propellant::Methane,
            Propellant::UDMH, Propellant::NTO, Propellant::SolidMix,
        ] {
            assert!(
                Propellant::LH2.density_kg_per_l() < p.density_kg_per_l(),
                "LH2 should be less dense than {:?}", p
            );
        }
    }
}
