/// A prototype is a pre-defined set of tags for a common entity type.
#[derive(Debug, Clone)]
pub struct Prototype {
    pub name: &'static str,
    pub doc: &'static str,
    /// (tag_name, default_value_or_none_for_marker)
    pub tags: &'static [(&'static str, Option<&'static str>)],
}

// ----------------------------------------------------------------
// Equipment prototypes
// ----------------------------------------------------------------

pub static EQUIP_PROTOTYPES: &[Prototype] = &[
    Prototype {
        name: "ahu",
        doc: "Air Handling Unit",
        tags: &[("equip", None), ("ahu", None), ("air", None)],
    },
    Prototype {
        name: "ahu-singleDuct",
        doc: "Single Duct AHU",
        tags: &[
            ("equip", None),
            ("ahu", None),
            ("air", None),
            ("singleDuct", None),
        ],
    },
    Prototype {
        name: "ahu-dualDuct",
        doc: "Dual Duct AHU",
        tags: &[
            ("equip", None),
            ("ahu", None),
            ("air", None),
            ("dualDuct", None),
        ],
    },
    Prototype {
        name: "rtu",
        doc: "Rooftop Unit",
        tags: &[("equip", None), ("rtu", None), ("air", None)],
    },
    Prototype {
        name: "mau",
        doc: "Makeup Air Unit",
        tags: &[("equip", None), ("mau", None), ("air", None)],
    },
    Prototype {
        name: "fcu",
        doc: "Fan Coil Unit",
        tags: &[("equip", None), ("fcu", None), ("air", None)],
    },
    Prototype {
        name: "vav",
        doc: "Variable Air Volume box",
        tags: &[
            ("equip", None),
            ("vav", None),
            ("air", None),
            ("variableVolume", None),
        ],
    },
    Prototype {
        name: "vav-reheat",
        doc: "VAV with Reheat",
        tags: &[
            ("equip", None),
            ("vav", None),
            ("air", None),
            ("variableVolume", None),
            ("reheat", None),
        ],
    },
    Prototype {
        name: "boiler",
        doc: "Boiler",
        tags: &[
            ("equip", None),
            ("boiler", None),
            ("water", None),
            ("hot", None),
            ("heating", None),
        ],
    },
    Prototype {
        name: "chiller",
        doc: "Chiller",
        tags: &[
            ("equip", None),
            ("chiller", None),
            ("water", None),
            ("chilled", None),
            ("cooling", None),
        ],
    },
    Prototype {
        name: "coolingTower",
        doc: "Cooling Tower",
        tags: &[
            ("equip", None),
            ("coolingTower", None),
            ("water", None),
            ("condenser", None),
        ],
    },
    Prototype {
        name: "pump",
        doc: "Pump",
        tags: &[("equip", None), ("pump", None)],
    },
    Prototype {
        name: "pump-chw",
        doc: "Chilled Water Pump",
        tags: &[
            ("equip", None),
            ("pump", None),
            ("water", None),
            ("chilled", None),
        ],
    },
    Prototype {
        name: "pump-hw",
        doc: "Hot Water Pump",
        tags: &[
            ("equip", None),
            ("pump", None),
            ("water", None),
            ("hot", None),
        ],
    },
    Prototype {
        name: "pump-cw",
        doc: "Condenser Water Pump",
        tags: &[
            ("equip", None),
            ("pump", None),
            ("water", None),
            ("condenser", None),
        ],
    },
    Prototype {
        name: "fan",
        doc: "Fan",
        tags: &[("equip", None), ("fan", None), ("air", None)],
    },
    Prototype {
        name: "fan-supply",
        doc: "Supply Fan",
        tags: &[
            ("equip", None),
            ("fan", None),
            ("air", None),
            ("supply", None),
        ],
    },
    Prototype {
        name: "fan-return",
        doc: "Return Fan",
        tags: &[
            ("equip", None),
            ("fan", None),
            ("air", None),
            ("return", None),
        ],
    },
    Prototype {
        name: "fan-exhaust",
        doc: "Exhaust Fan",
        tags: &[
            ("equip", None),
            ("fan", None),
            ("air", None),
            ("exhaust", None),
        ],
    },
    Prototype {
        name: "damper",
        doc: "Damper",
        tags: &[("equip", None), ("damper", None), ("air", None)],
    },
    Prototype {
        name: "damper-outside",
        doc: "Outside Air Damper",
        tags: &[
            ("equip", None),
            ("damper", None),
            ("air", None),
            ("outside", None),
        ],
    },
    Prototype {
        name: "valve",
        doc: "Valve",
        tags: &[("equip", None), ("valve", None)],
    },
    Prototype {
        name: "valve-hw",
        doc: "Hot Water Valve",
        tags: &[
            ("equip", None),
            ("valve", None),
            ("water", None),
            ("hot", None),
        ],
    },
    Prototype {
        name: "valve-chw",
        doc: "Chilled Water Valve",
        tags: &[
            ("equip", None),
            ("valve", None),
            ("water", None),
            ("chilled", None),
        ],
    },
    Prototype {
        name: "meter-elec",
        doc: "Electric Meter",
        tags: &[("equip", None), ("meter", None), ("elec", None)],
    },
    Prototype {
        name: "meter-gas",
        doc: "Gas Meter",
        tags: &[("equip", None), ("meter", None), ("gas", None)],
    },
    Prototype {
        name: "meter-water",
        doc: "Water Meter",
        tags: &[("equip", None), ("meter", None), ("water", None)],
    },
    Prototype {
        name: "heatPump",
        doc: "Heat Pump",
        tags: &[("equip", None), ("heatPump", None)],
    },
    Prototype {
        name: "thermostat",
        doc: "Thermostat",
        tags: &[("equip", None), ("thermostat", None)],
    },
    Prototype {
        name: "vfd",
        doc: "Variable Frequency Drive",
        tags: &[("equip", None), ("vfd", None)],
    },
    Prototype {
        name: "panel",
        doc: "Electrical Panel",
        tags: &[("equip", None), ("panel", None), ("elec", None)],
    },
    Prototype {
        name: "ups",
        doc: "UPS",
        tags: &[("equip", None), ("ups", None), ("elec", None)],
    },
    Prototype {
        name: "heatExchanger",
        doc: "Heat Exchanger",
        tags: &[("equip", None), ("heatExchanger", None)],
    },
    Prototype {
        name: "humidifier",
        doc: "Humidifier",
        tags: &[("equip", None), ("humidifier", None)],
    },
    Prototype {
        name: "filter",
        doc: "Air Filter",
        tags: &[("equip", None), ("filter", None), ("air", None)],
    },
];

// ----------------------------------------------------------------
// Point prototypes
// ----------------------------------------------------------------

pub static POINT_PROTOTYPES: &[Prototype] = &[
    // Air temperature points
    Prototype {
        name: "discharge-air-temp-sensor",
        doc: "Discharge air temperature sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("discharge", None),
            ("air", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "return-air-temp-sensor",
        doc: "Return air temperature sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("return", None),
            ("air", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "mixed-air-temp-sensor",
        doc: "Mixed air temperature sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("mixed", None),
            ("air", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "outside-air-temp-sensor",
        doc: "Outside air temperature sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("outside", None),
            ("air", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "zone-air-temp-sensor",
        doc: "Zone air temperature sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("zone", None),
            ("air", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "zone-air-temp-sp",
        doc: "Zone air temperature setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("zone", None),
            ("air", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "zone-air-temp-heating-sp",
        doc: "Zone heating setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("zone", None),
            ("air", None),
            ("temp", None),
            ("heating", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "zone-air-temp-cooling-sp",
        doc: "Zone cooling setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("zone", None),
            ("air", None),
            ("temp", None),
            ("cooling", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "discharge-air-temp-sp",
        doc: "Discharge air temperature setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("discharge", None),
            ("air", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // Air pressure / flow
    Prototype {
        name: "discharge-air-pressure-sensor",
        doc: "Duct static pressure sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("discharge", None),
            ("air", None),
            ("pressure", None),
            ("kind", Some("Number")),
            ("unit", Some("inH₂O")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "discharge-air-pressure-sp",
        doc: "Duct static pressure setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("discharge", None),
            ("air", None),
            ("pressure", None),
            ("kind", Some("Number")),
            ("unit", Some("inH₂O")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "discharge-air-flow-sensor",
        doc: "Discharge airflow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("discharge", None),
            ("air", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfm")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "discharge-air-flow-sp",
        doc: "Discharge airflow setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("discharge", None),
            ("air", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfm")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // Humidity
    Prototype {
        name: "zone-air-humidity-sensor",
        doc: "Zone humidity sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("zone", None),
            ("air", None),
            ("humidity", None),
            ("kind", Some("Number")),
            ("unit", Some("%RH")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "return-air-humidity-sensor",
        doc: "Return air humidity sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("return", None),
            ("air", None),
            ("humidity", None),
            ("kind", Some("Number")),
            ("unit", Some("%RH")),
            ("cur", None),
            ("his", None),
        ],
    },
    // CO₂
    Prototype {
        name: "zone-air-co2-sensor",
        doc: "Zone CO₂ sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("zone", None),
            ("air", None),
            ("co2", None),
            ("kind", Some("Number")),
            ("unit", Some("ppm")),
            ("cur", None),
            ("his", None),
        ],
    },
    // Damper commands
    Prototype {
        name: "outside-air-damper-cmd",
        doc: "Outside air damper command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("outside", None),
            ("air", None),
            ("damper", None),
            ("damperPosition", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // Valve commands
    Prototype {
        name: "hot-water-valve-cmd",
        doc: "Hot water valve command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("hot", None),
            ("water", None),
            ("valve", None),
            ("valvePosition", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "chilled-water-valve-cmd",
        doc: "Chilled water valve command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("chilled", None),
            ("water", None),
            ("valve", None),
            ("valvePosition", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // Fan / equipment run commands
    Prototype {
        name: "fan-run-cmd",
        doc: "Fan run command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("fan", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "fan-run-sensor",
        doc: "Fan run status",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("fan", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "fan-speed-cmd",
        doc: "Fan speed command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("fan", None),
            ("speed", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "fan-speed-sensor",
        doc: "Fan speed feedback",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("fan", None),
            ("speed", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("his", None),
        ],
    },
    // Pump
    Prototype {
        name: "pump-run-cmd",
        doc: "Pump run command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("pump", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "pump-run-sensor",
        doc: "Pump run status",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("pump", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("his", None),
        ],
    },
    // Water temps
    Prototype {
        name: "chilled-water-entering-temp-sensor",
        doc: "Chilled water entering temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("chilled", None),
            ("water", None),
            ("entering", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "chilled-water-leaving-temp-sensor",
        doc: "Chilled water leaving temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("chilled", None),
            ("water", None),
            ("leaving", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "hot-water-entering-temp-sensor",
        doc: "Hot water entering temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("hot", None),
            ("water", None),
            ("entering", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "hot-water-leaving-temp-sensor",
        doc: "Hot water leaving temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("hot", None),
            ("water", None),
            ("leaving", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    // Electrical metering points
    Prototype {
        name: "elec-power-sensor",
        doc: "Electric power sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("power", None),
            ("kind", Some("Number")),
            ("unit", Some("kW")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "elec-energy-sensor",
        doc: "Electric energy sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("energy", None),
            ("kind", Some("Number")),
            ("unit", Some("kWh")),
            ("cur", None),
            ("his", None),
        ],
    },
    // Occupancy
    Prototype {
        name: "zone-occupied-sensor",
        doc: "Zone occupancy sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("zone", None),
            ("occupied", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("his", None),
        ],
    },
    // Enable / mode
    Prototype {
        name: "equip-enable-cmd",
        doc: "Equipment enable command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("enable", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "occ-cmd",
        doc: "Occupied mode command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("occ", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // Alarm / fault status
    Prototype {
        name: "equip-alarm-sensor",
        doc: "Equipment alarm status",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("alarm", None),
            ("kind", Some("Bool")),
            ("cur", None),
        ],
    },
    Prototype {
        name: "equip-fault-sensor",
        doc: "Equipment fault status",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("fault", None),
            ("kind", Some("Bool")),
            ("cur", None),
        ],
    },
    // Filter DP
    Prototype {
        name: "filter-dp-sensor",
        doc: "Filter differential pressure sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("filterDp", None),
            ("pressure", None),
            ("kind", Some("Number")),
            ("unit", Some("inH₂O")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Supply / return air damper + flow
    // ----------------------------------------------------------------
    Prototype {
        name: "return-air-damper-cmd",
        doc: "Return air damper command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("return", None),
            ("air", None),
            ("damper", None),
            ("damperPosition", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "exhaust-air-damper-cmd",
        doc: "Exhaust air damper command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("exhaust", None),
            ("air", None),
            ("damper", None),
            ("damperPosition", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "return-air-flow-sensor",
        doc: "Return airflow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("return", None),
            ("air", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfm")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "outside-air-flow-sensor",
        doc: "Outside airflow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("outside", None),
            ("air", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfm")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "outside-air-flow-sp",
        doc: "Minimum outside airflow setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("outside", None),
            ("air", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfm")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // ----------------------------------------------------------------
    // VAV-specific points
    // ----------------------------------------------------------------
    Prototype {
        name: "vav-air-flow-sensor",
        doc: "VAV box airflow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("air", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfm")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "vav-air-flow-sp",
        doc: "VAV box airflow setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("air", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfm")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "vav-damper-cmd",
        doc: "VAV damper position command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("air", None),
            ("damper", None),
            ("damperPosition", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "vav-reheat-valve-cmd",
        doc: "VAV reheat valve command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("hot", None),
            ("water", None),
            ("valve", None),
            ("reheat", None),
            ("valvePosition", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "vav-heating-sp",
        doc: "VAV heating setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("air", None),
            ("temp", None),
            ("heating", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "vav-cooling-sp",
        doc: "VAV cooling setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("air", None),
            ("temp", None),
            ("cooling", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // ----------------------------------------------------------------
    // Chiller / plant points
    // ----------------------------------------------------------------
    Prototype {
        name: "chiller-run-cmd",
        doc: "Chiller run command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("chiller", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "chiller-run-sensor",
        doc: "Chiller run status",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("chiller", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "chilled-water-supply-temp-sp",
        doc: "Chilled water supply temperature setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("chilled", None),
            ("water", None),
            ("leaving", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "condenser-water-entering-temp-sensor",
        doc: "Condenser water entering temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("condenser", None),
            ("water", None),
            ("entering", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "condenser-water-leaving-temp-sensor",
        doc: "Condenser water leaving temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("condenser", None),
            ("water", None),
            ("leaving", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "chilled-water-flow-sensor",
        doc: "Chilled water flow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("chilled", None),
            ("water", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("gpm")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "chilled-water-dp-sensor",
        doc: "Chilled water differential pressure sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("chilled", None),
            ("water", None),
            ("pressure", None),
            ("differential", None),
            ("kind", Some("Number")),
            ("unit", Some("psi")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "chilled-water-dp-sp",
        doc: "Chilled water differential pressure setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("chilled", None),
            ("water", None),
            ("pressure", None),
            ("differential", None),
            ("kind", Some("Number")),
            ("unit", Some("psi")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // ----------------------------------------------------------------
    // Boiler points
    // ----------------------------------------------------------------
    Prototype {
        name: "boiler-run-cmd",
        doc: "Boiler run command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("boiler", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "boiler-run-sensor",
        doc: "Boiler run status",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("boiler", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "hot-water-supply-temp-sp",
        doc: "Hot water supply temperature setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("hot", None),
            ("water", None),
            ("leaving", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "hot-water-flow-sensor",
        doc: "Hot water flow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("hot", None),
            ("water", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("gpm")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "hot-water-dp-sensor",
        doc: "Hot water differential pressure sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("hot", None),
            ("water", None),
            ("pressure", None),
            ("differential", None),
            ("kind", Some("Number")),
            ("unit", Some("psi")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Cooling tower points
    // ----------------------------------------------------------------
    Prototype {
        name: "cooling-tower-fan-cmd",
        doc: "Cooling tower fan command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("coolingTower", None),
            ("fan", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "cooling-tower-fan-speed-cmd",
        doc: "Cooling tower fan speed command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("coolingTower", None),
            ("fan", None),
            ("speed", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "condenser-water-supply-temp-sp",
        doc: "Condenser water supply temperature setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("condenser", None),
            ("water", None),
            ("leaving", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // ----------------------------------------------------------------
    // VFD points
    // ----------------------------------------------------------------
    Prototype {
        name: "vfd-speed-cmd",
        doc: "VFD speed command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("vfd", None),
            ("speed", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "vfd-speed-sensor",
        doc: "VFD speed feedback",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("vfd", None),
            ("speed", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "vfd-freq-sensor",
        doc: "VFD output frequency",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("vfd", None),
            ("freq", None),
            ("kind", Some("Number")),
            ("unit", Some("Hz")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "vfd-current-sensor",
        doc: "VFD motor current",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("vfd", None),
            ("current", None),
            ("elec", None),
            ("kind", Some("Number")),
            ("unit", Some("A")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Electrical metering — additional
    // ----------------------------------------------------------------
    Prototype {
        name: "elec-demand-sensor",
        doc: "Electric demand sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("demand", None),
            ("kind", Some("Number")),
            ("unit", Some("kW")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "elec-voltage-sensor",
        doc: "Electric voltage sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("volt", None),
            ("kind", Some("Number")),
            ("unit", Some("V")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "elec-current-sensor",
        doc: "Electric current sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("current", None),
            ("kind", Some("Number")),
            ("unit", Some("A")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "elec-pf-sensor",
        doc: "Electric power factor sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("pf", None),
            ("kind", Some("Number")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "elec-reactive-power-sensor",
        doc: "Electric reactive power sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("reactive", None),
            ("power", None),
            ("kind", Some("Number")),
            ("unit", Some("kVAR")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "elec-apparent-power-sensor",
        doc: "Electric apparent power sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("elec", None),
            ("apparent", None),
            ("power", None),
            ("kind", Some("Number")),
            ("unit", Some("kVA")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Gas / water metering
    // ----------------------------------------------------------------
    Prototype {
        name: "gas-flow-sensor",
        doc: "Natural gas flow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("gas", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("cfh")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "gas-energy-sensor",
        doc: "Natural gas energy sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("gas", None),
            ("energy", None),
            ("kind", Some("Number")),
            ("unit", Some("therms")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "water-flow-sensor",
        doc: "Water flow sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("water", None),
            ("flow", None),
            ("kind", Some("Number")),
            ("unit", Some("gpm")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "water-volume-sensor",
        doc: "Water volume / consumption sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("water", None),
            ("volume", None),
            ("kind", Some("Number")),
            ("unit", Some("gal")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Heating / cooling mode + stages
    // ----------------------------------------------------------------
    Prototype {
        name: "heating-cmd",
        doc: "Heating command / output",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("heating", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "cooling-cmd",
        doc: "Cooling command / output",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("cooling", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "heating-stage-cmd",
        doc: "Heating stage command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("heating", None),
            ("stage", None),
            ("kind", Some("Number")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "cooling-stage-cmd",
        doc: "Cooling stage command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("cooling", None),
            ("stage", None),
            ("kind", Some("Number")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "hvac-mode-cmd",
        doc: "HVAC mode (off/heat/cool/auto)",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("hvacMode", None),
            ("kind", Some("Str")),
            ("cur", None),
            ("writable", None),
        ],
    },
    // ----------------------------------------------------------------
    // Thermostat points
    // ----------------------------------------------------------------
    Prototype {
        name: "thermostat-temp-sensor",
        doc: "Thermostat room temperature sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("zone", None),
            ("air", None),
            ("temp", None),
            ("thermostat", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "thermostat-heating-sp",
        doc: "Thermostat heating setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("zone", None),
            ("air", None),
            ("temp", None),
            ("heating", None),
            ("thermostat", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "thermostat-cooling-sp",
        doc: "Thermostat cooling setpoint",
        tags: &[
            ("point", None),
            ("sp", None),
            ("zone", None),
            ("air", None),
            ("temp", None),
            ("cooling", None),
            ("thermostat", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "thermostat-humidity-sensor",
        doc: "Thermostat humidity sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("zone", None),
            ("air", None),
            ("humidity", None),
            ("thermostat", None),
            ("kind", Some("Number")),
            ("unit", Some("%RH")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Lighting
    // ----------------------------------------------------------------
    Prototype {
        name: "lights-level-cmd",
        doc: "Lighting level command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("lights", None),
            ("level", None),
            ("kind", Some("Number")),
            ("unit", Some("%")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "lights-run-cmd",
        doc: "Lighting on/off command",
        tags: &[
            ("point", None),
            ("cmd", None),
            ("lights", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("writable", None),
        ],
    },
    Prototype {
        name: "lights-run-sensor",
        doc: "Lighting on/off status",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("lights", None),
            ("run", None),
            ("kind", Some("Bool")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "lux-sensor",
        doc: "Ambient light level sensor",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("light", None),
            ("illuminance", None),
            ("kind", Some("Number")),
            ("unit", Some("lux")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Refrigeration
    // ----------------------------------------------------------------
    Prototype {
        name: "refrig-suction-temp-sensor",
        doc: "Refrigerant suction temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("refrig", None),
            ("suction", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "refrig-suction-pressure-sensor",
        doc: "Refrigerant suction pressure",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("refrig", None),
            ("suction", None),
            ("pressure", None),
            ("kind", Some("Number")),
            ("unit", Some("psi")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "refrig-discharge-temp-sensor",
        doc: "Refrigerant discharge temperature",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("refrig", None),
            ("discharge", None),
            ("temp", None),
            ("kind", Some("Number")),
            ("unit", Some("°F")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "refrig-discharge-pressure-sensor",
        doc: "Refrigerant discharge pressure",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("refrig", None),
            ("discharge", None),
            ("pressure", None),
            ("kind", Some("Number")),
            ("unit", Some("psi")),
            ("cur", None),
            ("his", None),
        ],
    },
    // ----------------------------------------------------------------
    // Runtime / maintenance
    // ----------------------------------------------------------------
    Prototype {
        name: "run-hours-sensor",
        doc: "Equipment runtime hours",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("run", None),
            ("duration", None),
            ("kind", Some("Number")),
            ("unit", Some("hr")),
            ("cur", None),
            ("his", None),
        ],
    },
    Prototype {
        name: "starts-sensor",
        doc: "Equipment start count",
        tags: &[
            ("point", None),
            ("sensor", None),
            ("run", None),
            ("starts", None),
            ("kind", Some("Number")),
            ("cur", None),
            ("his", None),
        ],
    },
];

// ----------------------------------------------------------------
// Lookup
// ----------------------------------------------------------------

/// Find an equipment prototype by name.
pub fn find_equip_prototype(name: &str) -> Option<&'static Prototype> {
    EQUIP_PROTOTYPES.iter().find(|p| p.name == name)
}

/// Find a point prototype by name.
pub fn find_point_prototype(name: &str) -> Option<&'static Prototype> {
    POINT_PROTOTYPES.iter().find(|p| p.name == name)
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equip_prototypes_have_equip_tag() {
        for proto in EQUIP_PROTOTYPES {
            assert!(
                proto.tags.iter().any(|&(name, _)| name == "equip"),
                "equip prototype '{}' missing equip marker",
                proto.name
            );
        }
    }

    #[test]
    fn point_prototypes_have_point_tag() {
        for proto in POINT_PROTOTYPES {
            assert!(
                proto.tags.iter().any(|&(name, _)| name == "point"),
                "point prototype '{}' missing point marker",
                proto.name
            );
        }
    }

    #[test]
    fn point_prototypes_have_classification() {
        for proto in POINT_PROTOTYPES {
            let has_class = proto
                .tags
                .iter()
                .any(|&(name, _)| name == "sensor" || name == "cmd" || name == "sp");
            assert!(
                has_class,
                "point prototype '{}' missing sensor/cmd/sp",
                proto.name
            );
        }
    }

    #[test]
    fn find_ahu_prototype() {
        assert!(find_equip_prototype("ahu").is_some());
        assert!(find_equip_prototype("nonexistent").is_none());
    }

    #[test]
    fn find_dat_sensor_prototype() {
        let proto = find_point_prototype("discharge-air-temp-sensor").unwrap();
        let tag_names: Vec<&str> = proto.tags.iter().map(|&(n, _)| n).collect();
        assert!(tag_names.contains(&"discharge"));
        assert!(tag_names.contains(&"air"));
        assert!(tag_names.contains(&"temp"));
        assert!(tag_names.contains(&"sensor"));
    }
}
