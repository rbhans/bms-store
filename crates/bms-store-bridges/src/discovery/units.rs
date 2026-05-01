//! Unit normalization for BMS point discovery.
//!
//! Maps common vendor-specific and informal unit strings to canonical Haystack / SI forms.
//! `normalize_unit` is idempotent: calling it twice gives the same result as calling once.

/// Normalize a raw unit string to its canonical BMS form.
///
/// - Case-insensitive match.
/// - Unknown strings are returned unchanged (lowercased).
/// - Idempotent: `normalize_unit(normalize_unit(x)) == normalize_unit(x)`.
pub fn normalize_unit(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lower = trimmed.to_lowercase();

    // ── Temperature ──────────────────────────────────────────────
    match lower.as_str() {
        "deg f" | "degf" | "°f" | "deg_f" | "degrees f" | "f" => return "°F".to_string(),
        "deg c" | "degc" | "°c" | "deg_c" | "degrees c" | "c" | "celsius" => {
            return "°C".to_string()
        }
        "deg k" | "degk" | "°k" | "kelvin" | "k" => return "K".to_string(),
        _ => {}
    }

    // ── Pressure ─────────────────────────────────────────────────
    match lower.as_str() {
        "psi" | "psig" | "psig." => return "psi".to_string(),
        "psia" => return "psia".to_string(),
        "kpa" => return "kPa".to_string(),
        "mpa" => return "MPa".to_string(),
        "bar" => return "bar".to_string(),
        "mbar" | "millibar" => return "mbar".to_string(),
        "in wc" | "inwc" | "in. wc" | "inh2o" | "in h2o" | "in.wc" | "in_wc" | "\"wc"
        | "inches wc" | "in water" => return "inH₂O".to_string(),
        "pa" | "pascals" | "pascal" => return "Pa".to_string(),
        _ => {}
    }

    // ── Flow — air ───────────────────────────────────────────────
    match lower.as_str() {
        "cfm" | "ft3/min" | "ft³/min" | "cu ft/min" | "cubic feet per minute"
        | "ft^3/min" => return "cfm".to_string(),
        "cfh" | "ft3/hr" | "ft³/hr" | "cu ft/hr" => return "cfh".to_string(),
        "l/s" | "ls" | "lps" | "liters/sec" | "litres/sec" => return "L/s".to_string(),
        "m3/s" | "m³/s" => return "m³/s".to_string(),
        "m3/h" | "m³/h" | "m3/hr" | "m³/hr" => return "m³/h".to_string(),
        _ => {}
    }

    // ── Flow — liquid ────────────────────────────────────────────
    match lower.as_str() {
        "gpm" | "gal/min" | "gal/m" | "gallons/min" | "gallons per minute" => {
            return "gpm".to_string()
        }
        "gph" | "gal/hr" | "gallons/hr" => return "gph".to_string(),
        "lpm" | "l/min" | "liters/min" | "litres/min" => return "L/min".to_string(),
        _ => {}
    }

    // ── Percentage ───────────────────────────────────────────────
    match lower.as_str() {
        "%" | "pct" | "percent" | "percentage" => return "%".to_string(),
        "%rh" | "% rh" | "rh" | "% rel. humidity" | "%rel.hum" => return "%RH".to_string(),
        _ => {}
    }

    // ── Energy ───────────────────────────────────────────────────
    match lower.as_str() {
        "kwh" | "kw*h" | "kilowatt-hour" | "kilowatt hour" => return "kWh".to_string(),
        "mwh" | "megawatt-hour" => return "MWh".to_string(),
        "btu" => return "BTU".to_string(),
        "mbtu" | "mmbtu" => return "MBTU".to_string(),
        "kbtu" => return "kBTU".to_string(),
        "therms" | "therm" => return "therms".to_string(),
        "j" | "joules" | "joule" => return "J".to_string(),
        "kj" | "kilojoules" => return "kJ".to_string(),
        "mj" | "megajoules" => return "MJ".to_string(),
        "wh" | "w*h" | "watt-hour" => return "Wh".to_string(),
        _ => {}
    }

    // ── Power ────────────────────────────────────────────────────
    match lower.as_str() {
        "kw" | "kilowatt" | "kilowatts" => return "kW".to_string(),
        "mw" | "megawatt" | "megawatts" => return "MW".to_string(),
        "w" | "watt" | "watts" => return "W".to_string(),
        "kva" | "kilo-volt-ampere" => return "kVA".to_string(),
        "kvar" | "kilo-volt-ampere-reactive" => return "kVAR".to_string(),
        "va" | "volt-ampere" => return "VA".to_string(),
        "var" | "volt-ampere-reactive" => return "VAR".to_string(),
        "hp" | "horsepower" => return "hp".to_string(),
        _ => {}
    }

    // ── Electrical ───────────────────────────────────────────────
    match lower.as_str() {
        "v" | "volt" | "volts" | "vac" | "vdc" => return "V".to_string(),
        "a" | "amp" | "amps" | "ampere" | "amperes" => return "A".to_string(),
        "ma" | "milliamp" | "milliamps" | "milliampere" => return "mA".to_string(),
        "ohm" | "ohms" | "ω" => return "Ω".to_string(),
        "hz" | "hertz" => return "Hz".to_string(),
        "khz" => return "kHz".to_string(),
        "mhz" => return "MHz".to_string(),
        _ => {}
    }

    // ── Concentration ────────────────────────────────────────────
    match lower.as_str() {
        "ppm" | "parts per million" => return "ppm".to_string(),
        "ppb" | "parts per billion" => return "ppb".to_string(),
        "mg/m3" | "mg/m³" => return "mg/m³".to_string(),
        _ => {}
    }

    // ── Illuminance / lighting ───────────────────────────────────
    match lower.as_str() {
        "lux" | "lx" => return "lux".to_string(),
        "fc" | "footcandle" | "foot-candle" | "foot candle" => return "fc".to_string(),
        _ => {}
    }

    // ── Time ─────────────────────────────────────────────────────
    match lower.as_str() {
        "s" | "sec" | "seconds" | "second" => return "s".to_string(),
        "min" | "minutes" | "minute" => return "min".to_string(),
        "h" | "hr" | "hrs" | "hour" | "hours" => return "hr".to_string(),
        "days" | "day" | "d" => return "days".to_string(),
        _ => {}
    }

    // ── Volume ───────────────────────────────────────────────────
    match lower.as_str() {
        "gal" | "gallon" | "gallons" => return "gal".to_string(),
        "l" | "liter" | "liters" | "litre" | "litres" => return "L".to_string(),
        "m3" | "m³" | "cubic meters" | "cubic meter" => return "m³".to_string(),
        "ft3" | "ft³" | "cubic feet" | "cubic foot" => return "ft³".to_string(),
        _ => {}
    }

    // ── Length / area ────────────────────────────────────────────
    match lower.as_str() {
        "m" | "meters" | "meter" | "metre" | "metres" => return "m".to_string(),
        "ft" | "feet" | "foot" => return "ft".to_string(),
        "in" | "inch" | "inches" => return "in".to_string(),
        "m2" | "m²" | "sq m" | "sqm" => return "m²".to_string(),
        "ft2" | "ft²" | "sq ft" | "sqft" => return "ft²".to_string(),
        _ => {}
    }

    // ── Mass ─────────────────────────────────────────────────────
    match lower.as_str() {
        "kg" | "kilograms" | "kilogram" => return "kg".to_string(),
        "lb" | "lbs" | "pound" | "pounds" => return "lb".to_string(),
        "g" | "gram" | "grams" => return "g".to_string(),
        _ => {}
    }

    // ── Dimensionless / boolean ───────────────────────────────────
    match lower.as_str() {
        "none" | "dimensionless" | "unitless" | "-" | "" => return String::new(),
        "bool" | "boolean" | "on/off" | "true/false" => return String::new(),
        _ => {}
    }

    // Unknown: return as-is (trimmed).
    trimmed.to_string()
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_variants() {
        assert_eq!(normalize_unit("deg F"), "°F");
        assert_eq!(normalize_unit("°F"), "°F");
        assert_eq!(normalize_unit("degF"), "°F");
        assert_eq!(normalize_unit("DEG F"), "°F");
        assert_eq!(normalize_unit("F"), "°F");
        assert_eq!(normalize_unit("deg C"), "°C");
        assert_eq!(normalize_unit("°C"), "°C");
        assert_eq!(normalize_unit("degC"), "°C");
    }

    #[test]
    fn pressure_variants() {
        assert_eq!(normalize_unit("psi"), "psi");
        assert_eq!(normalize_unit("PSI"), "psi");
        assert_eq!(normalize_unit("psig"), "psi");
        assert_eq!(normalize_unit("kpa"), "kPa");
        assert_eq!(normalize_unit("kPa"), "kPa");
        assert_eq!(normalize_unit("KPA"), "kPa");
        assert_eq!(normalize_unit("in wc"), "inH₂O");
        assert_eq!(normalize_unit("inWC"), "inH₂O");
        assert_eq!(normalize_unit("in. WC"), "inH₂O");
        assert_eq!(normalize_unit("inH2O"), "inH₂O");
    }

    #[test]
    fn flow_variants() {
        assert_eq!(normalize_unit("cfm"), "cfm");
        assert_eq!(normalize_unit("CFM"), "cfm");
        assert_eq!(normalize_unit("ft3/min"), "cfm");
        assert_eq!(normalize_unit("ft³/min"), "cfm");
        assert_eq!(normalize_unit("gpm"), "gpm");
        assert_eq!(normalize_unit("GPM"), "gpm");
        assert_eq!(normalize_unit("gal/min"), "gpm");
    }

    #[test]
    fn percentage_variants() {
        assert_eq!(normalize_unit("%"), "%");
        assert_eq!(normalize_unit("pct"), "%");
        assert_eq!(normalize_unit("percent"), "%");
        assert_eq!(normalize_unit("%RH"), "%RH");
        assert_eq!(normalize_unit("rh"), "%RH");
    }

    #[test]
    fn power_energy_variants() {
        assert_eq!(normalize_unit("kW"), "kW");
        assert_eq!(normalize_unit("kw"), "kW");
        assert_eq!(normalize_unit("KW"), "kW");
        assert_eq!(normalize_unit("kWh"), "kWh");
        assert_eq!(normalize_unit("kwh"), "kWh");
        assert_eq!(normalize_unit("W"), "W");
        assert_eq!(normalize_unit("watt"), "W");
    }

    #[test]
    fn electrical_variants() {
        assert_eq!(normalize_unit("V"), "V");
        assert_eq!(normalize_unit("volt"), "V");
        assert_eq!(normalize_unit("VAC"), "V");
        assert_eq!(normalize_unit("A"), "A");
        assert_eq!(normalize_unit("amp"), "A");
        assert_eq!(normalize_unit("Hz"), "Hz");
        assert_eq!(normalize_unit("hertz"), "Hz");
    }

    #[test]
    fn time_variants() {
        assert_eq!(normalize_unit("hr"), "hr");
        assert_eq!(normalize_unit("hours"), "hr");
        assert_eq!(normalize_unit("min"), "min");
        assert_eq!(normalize_unit("s"), "s");
        assert_eq!(normalize_unit("seconds"), "s");
    }

    #[test]
    fn unknown_passes_through() {
        // Completely unknown units are returned as-is (trimmed).
        assert_eq!(normalize_unit("brix"), "brix");
        assert_eq!(normalize_unit("SCFM"), "SCFM");
    }

    #[test]
    fn empty_is_empty() {
        assert_eq!(normalize_unit(""), "");
        assert_eq!(normalize_unit("  "), "");
        assert_eq!(normalize_unit("none"), "");
        assert_eq!(normalize_unit("dimensionless"), "");
    }

    #[test]
    fn idempotent_deg_f() {
        let once = normalize_unit("deg F");
        let twice = normalize_unit(&once);
        assert_eq!(once, twice, "normalize_unit must be idempotent");
    }

    #[test]
    fn idempotent_all_known() {
        let samples = &[
            "deg F", "°C", "psi", "kPa", "cfm", "gpm", "%", "inH₂O", "kW", "kWh", "V", "A",
            "Hz", "ppm", "lux", "hr", "gal", "m³",
        ];
        for &s in samples {
            let once = normalize_unit(s);
            let twice = normalize_unit(&once);
            assert_eq!(once, twice, "not idempotent for '{s}': {once} → {twice}");
        }
    }
}
