/// Map a BACnet engineering unit ID (ASHRAE 135 Annex K) to a display string.
/// Returns `None` for unknown/unmapped unit IDs.
pub fn bacnet_unit_to_string(unit_id: u32) -> Option<&'static str> {
    match unit_id {
        // Temperature
        62 => Some("°C"),
        64 => Some("°F"),
        65 => Some("K"),

        // Pressure
        6 => Some("Pa"),
        7 => Some("kPa"),
        8 => Some("bar"),
        56 => Some("psi"),
        57 => Some("cmH₂O"),
        58 => Some("inH₂O"),
        59 => Some("mmHg"),
        60 => Some("inHg"),

        // Flow
        84 => Some("cfm"),
        85 => Some("L/s"),
        86 => Some("m³/s"),
        142 => Some("gpm"),
        143 => Some("L/min"),

        // Electrical
        19 => Some("W"),
        20 => Some("kW"),
        21 => Some("MW"),
        35 => Some("kWh"),
        36 => Some("MWh"),
        40 => Some("V"),
        41 => Some("kV"),
        42 => Some("MV"),
        3 => Some("A"),
        4 => Some("mA"),
        122 => Some("VA"),
        123 => Some("kVA"),
        126 => Some("VAR"),
        127 => Some("kVAR"),

        // Humidity
        29 => Some("%RH"),

        // Percent
        98 => Some("%"),

        // Speed / frequency
        104 => Some("rpm"),
        27 => Some("Hz"),
        28 => Some("kHz"),

        // Time
        72 => Some("s"),
        73 => Some("min"),
        74 => Some("hr"),
        75 => Some("days"),

        // Volume
        128 => Some("gal"),
        129 => Some("L"),
        130 => Some("m³"),

        // Mass / weight
        110 => Some("kg"),
        111 => Some("lb"),

        // Length / distance
        116 => Some("m"),
        117 => Some("cm"),
        118 => Some("mm"),
        119 => Some("ft"),
        120 => Some("in"),

        // Energy
        22 => Some("BTU"),
        23 => Some("kBTU"),
        24 => Some("therm"),
        25 => Some("ton·hr"),

        // Power (thermal)
        26 => Some("BTU/hr"),

        // Concentration
        96 => Some("ppm"),
        97 => Some("ppb"),

        // Luminance
        37 => Some("lux"),
        38 => Some("lm"),

        // No units
        95 => Some(""),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_units() {
        assert_eq!(bacnet_unit_to_string(62), Some("°C"));
        assert_eq!(bacnet_unit_to_string(64), Some("°F"));
        assert_eq!(bacnet_unit_to_string(65), Some("K"));
        assert_eq!(bacnet_unit_to_string(56), Some("psi"));
        assert_eq!(bacnet_unit_to_string(84), Some("cfm"));
        assert_eq!(bacnet_unit_to_string(19), Some("W"));
        assert_eq!(bacnet_unit_to_string(20), Some("kW"));
        assert_eq!(bacnet_unit_to_string(35), Some("kWh"));
        assert_eq!(bacnet_unit_to_string(29), Some("%RH"));
        assert_eq!(bacnet_unit_to_string(98), Some("%"));
        assert_eq!(bacnet_unit_to_string(104), Some("rpm"));
        assert_eq!(bacnet_unit_to_string(142), Some("gpm"));
        assert_eq!(bacnet_unit_to_string(40), Some("V"));
        assert_eq!(bacnet_unit_to_string(3), Some("A"));
        assert_eq!(bacnet_unit_to_string(96), Some("ppm"));
    }

    #[test]
    fn unknown_unit() {
        assert_eq!(bacnet_unit_to_string(9999), None);
        assert_eq!(bacnet_unit_to_string(0), None);
    }

    #[test]
    fn no_units_marker() {
        assert_eq!(bacnet_unit_to_string(95), Some(""));
    }
}
