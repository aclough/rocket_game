//! Number formatters shared by every pane.

use super::*;

pub(super) fn format_dv(dv: f64) -> String {
    if dv.is_infinite() { "∞".to_string() }
    else { format!("{:.0} m/s", dv) }
}

/// Electrical power in human-readable units. Switches to kW above
/// 1 kW and MW above 1 MW so reactor outputs don't read as
/// "500000 W".
pub(super) fn format_power_w(w: f64) -> String {
    if w.abs() >= 1_000_000.0 {
        format!("{:.2} MW", w / 1_000_000.0)
    } else if w.abs() >= 10_000.0 {
        format!("{:.0} kW", w / 1_000.0)
    } else if w.abs() >= 1_000.0 {
        format!("{:.1} kW", w / 1_000.0)
    } else {
        format!("{:.0} W", w)
    }
}

/// Engine thrust in human-readable units. Ion thrusters live in the
/// 1 N range; chemical engines in kN; superheavy boosters in MN.
pub(super) fn format_thrust_n(n: f64) -> String {
    if n.abs() >= 1_000_000.0 {
        format!("{:.2} MN", n / 1_000_000.0)
    } else if n.abs() >= 10_000.0 {
        format!("{:.0} kN", n / 1_000.0)
    } else if n.abs() >= 1_000.0 {
        format!("{:.1} kN", n / 1_000.0)
    } else if n.abs() >= 10.0 {
        format!("{:.0} N", n)
    } else {
        format!("{:.1} N", n)
    }
}

/// Mass in kilograms with thousands-separator commas. Reactor masses
/// hit five+ figures at scale 1.0; the unspaced number is hard to read.
pub(super) fn format_kg(kg: f64) -> String {
    let int = kg.round() as i64;
    let sign = if int < 0 { "-" } else { "" };
    let mut digits = int.unsigned_abs().to_string();
    // Insert commas every 3 digits from the right.
    let mut i = digits.len();
    while i > 3 {
        i -= 3;
        digits.insert(i, ',');
    }
    format!("{}{} kg", sign, digits)
}

pub(super) fn format_flaw_rate(flaw: &Flaw) -> String {
    match flaw.trigger {
        FlawTrigger::PerFlight => format!("{:.0}%/flight", flaw.activation_chance * 100.0),
        FlawTrigger::PerDay => format!("{:.0}%/year, {:.2}%/day", flaw.activation_chance * 100.0, flaw.daily_rate() * 100.0),
    }
}

/// Truncate to `width` display columns, marking elision with `…`.
pub(super) fn fit(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(width - 1).collect();
    out.push('…');
    out
}

/// Format a mass in kg, switching to tons if >= 1000 kg.
pub(super) fn format_mass(kg: f64) -> String {
    if kg >= 1000.0 {
        format!("{:.1}t", kg / 1000.0)
    } else {
        format!("{:.0}kg", kg)
    }
}

/// Format an acceleration in m/s² as a multiple of standard gravity,
/// scaling down to mg / μg / ng for low-thrust craft.
pub(super) fn format_accel(a_m_s2: f64) -> String {
    let g = a_m_s2 / 9.80665;
    if g >= 1.0 {
        format!("{:.2} g", g)
    } else if g >= 1e-3 {
        format!("{:.0} mg", g * 1e3)
    } else if g >= 1e-6 {
        format!("{:.0} μg", g * 1e6)
    } else if g > 0.0 {
        format!("{:.0} ng", g * 1e9)
    } else {
        "0".to_string()
    }
}

pub(super) fn format_money_signed(amount: f64) -> String {
    if amount >= 0.0 {
        format!("+{}", format_money(amount))
    } else {
        format_money(amount)
    }
}


#[cfg(test)]
mod format_helpers_tests {
    use super::*;

    #[test]
    fn power_w_below_1k() {
        assert_eq!(format_power_w(500.0), "500 W");
        assert_eq!(format_power_w(999.0), "999 W");
    }

    #[test]
    fn power_kw_single_digit_keeps_decimal() {
        assert_eq!(format_power_w(1_000.0), "1.0 kW");
        assert_eq!(format_power_w(5_000.0), "5.0 kW");
        assert_eq!(format_power_w(9_999.0), "10.0 kW");
    }

    #[test]
    fn power_kw_double_digit_drops_decimal() {
        assert_eq!(format_power_w(10_000.0), "10 kW");
        assert_eq!(format_power_w(50_000.0), "50 kW");
        assert_eq!(format_power_w(500_000.0), "500 kW");
    }

    #[test]
    fn power_mw_uses_two_decimals() {
        assert_eq!(format_power_w(1_000_000.0), "1.00 MW");
        assert_eq!(format_power_w(5_500_000.0), "5.50 MW");
    }

    #[test]
    fn kg_with_commas() {
        assert_eq!(format_kg(0.0), "0 kg");
        assert_eq!(format_kg(800.0), "800 kg");
        assert_eq!(format_kg(4_200.0), "4,200 kg");
        assert_eq!(format_kg(33_348.0), "33,348 kg");
        assert_eq!(format_kg(1_234_567.0), "1,234,567 kg");
    }
}
