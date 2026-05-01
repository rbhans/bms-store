use serde::{Deserialize, Serialize};

// ----------------------------------------------------------------
// Tag kinds
// ----------------------------------------------------------------

/// What kind of value a tag carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    Marker,
    Bool,
    Number,
    Str,
    Ref,
    Uri,
    Date,
    Time,
    DateTime,
    Coord,
    List,
    Dict,
    Grid,
}

// ----------------------------------------------------------------
// Tag definition
// ----------------------------------------------------------------

/// A tag definition from the Haystack 4 ontology.
#[derive(Debug, Clone)]
pub struct TagDef {
    pub name: &'static str,
    pub kind: TagKind,
    pub doc: &'static str,
    pub supertype: Option<&'static str>,
    pub applies_to: &'static [&'static str],
}

// ----------------------------------------------------------------
// Static tag library
// ----------------------------------------------------------------

/// All Haystack 4 tags, organized by category.
pub static TAGS: &[TagDef] = &[
    // ============================================================
    // Entity markers
    // ============================================================
    TagDef {
        name: "site",
        kind: TagKind::Marker,
        doc: "Geographic site or campus",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "space",
        kind: TagKind::Marker,
        doc: "Enclosed space within a site",
        supertype: None,
        applies_to: &["space"],
    },
    TagDef {
        name: "equip",
        kind: TagKind::Marker,
        doc: "Physical or logical equipment",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "point",
        kind: TagKind::Marker,
        doc: "Data point (sensor, command, setpoint)",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "device",
        kind: TagKind::Marker,
        doc: "Networking device or controller",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "network",
        kind: TagKind::Marker,
        doc: "Communication network",
        supertype: None,
        applies_to: &["equip"],
    },
    // ============================================================
    // Point classification
    // ============================================================
    TagDef {
        name: "sensor",
        kind: TagKind::Marker,
        doc: "Sensor point (read-only input)",
        supertype: Some("point"),
        applies_to: &["point"],
    },
    TagDef {
        name: "cmd",
        kind: TagKind::Marker,
        doc: "Command point (writable output)",
        supertype: Some("point"),
        applies_to: &["point"],
    },
    TagDef {
        name: "sp",
        kind: TagKind::Marker,
        doc: "Setpoint",
        supertype: Some("point"),
        applies_to: &["point"],
    },
    // ============================================================
    // Substances
    // ============================================================
    TagDef {
        name: "air",
        kind: TagKind::Marker,
        doc: "Air substance",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "water",
        kind: TagKind::Marker,
        doc: "Water substance",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "elec",
        kind: TagKind::Marker,
        doc: "Electricity",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "gas",
        kind: TagKind::Marker,
        doc: "Natural gas",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "steam",
        kind: TagKind::Marker,
        doc: "Steam substance",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "refrig",
        kind: TagKind::Marker,
        doc: "Refrigerant",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "condensate",
        kind: TagKind::Marker,
        doc: "Condensate fluid",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "makeup",
        kind: TagKind::Marker,
        doc: "Makeup water or fluid",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "domestic",
        kind: TagKind::Marker,
        doc: "Domestic water",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "naturalGas",
        kind: TagKind::Marker,
        doc: "Natural gas fuel",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "fuelOil",
        kind: TagKind::Marker,
        doc: "Fuel oil",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "propane",
        kind: TagKind::Marker,
        doc: "Propane fuel",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "blowdown",
        kind: TagKind::Marker,
        doc: "Blowdown fluid discharge",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "flue",
        kind: TagKind::Marker,
        doc: "Flue gas exhaust",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    // ============================================================
    // Measurements / quantities
    // ============================================================
    TagDef {
        name: "temp",
        kind: TagKind::Marker,
        doc: "Temperature",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "humidity",
        kind: TagKind::Marker,
        doc: "Humidity",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "pressure",
        kind: TagKind::Marker,
        doc: "Pressure",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "flow",
        kind: TagKind::Marker,
        doc: "Flow rate",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "energy",
        kind: TagKind::Marker,
        doc: "Energy measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "power",
        kind: TagKind::Marker,
        doc: "Power measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "speed",
        kind: TagKind::Marker,
        doc: "Speed or velocity",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "freq",
        kind: TagKind::Marker,
        doc: "Frequency",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "volt",
        kind: TagKind::Marker,
        doc: "Voltage",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "current",
        kind: TagKind::Marker,
        doc: "Electrical current",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "co2",
        kind: TagKind::Marker,
        doc: "CO2 concentration",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "level",
        kind: TagKind::Marker,
        doc: "Level or percentage",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "occupied",
        kind: TagKind::Marker,
        doc: "Occupancy status",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "smoke",
        kind: TagKind::Marker,
        doc: "Smoke detection",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "damperPosition",
        kind: TagKind::Marker,
        doc: "Damper position",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "valvePosition",
        kind: TagKind::Marker,
        doc: "Valve position",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "differential",
        kind: TagKind::Marker,
        doc: "Differential measurement (e.g. differential pressure)",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "static",
        kind: TagKind::Marker,
        doc: "Static pressure measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "torque",
        kind: TagKind::Marker,
        doc: "Torque measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "vibration",
        kind: TagKind::Marker,
        doc: "Vibration measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "noise",
        kind: TagKind::Marker,
        doc: "Noise / sound level measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "lux",
        kind: TagKind::Marker,
        doc: "Illuminance measurement in lux",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "particulate",
        kind: TagKind::Marker,
        doc: "Particulate matter concentration",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "phLevel",
        kind: TagKind::Marker,
        doc: "pH level measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "conductivity",
        kind: TagKind::Marker,
        doc: "Electrical conductivity of fluid",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "turbidity",
        kind: TagKind::Marker,
        doc: "Turbidity measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "dissolved",
        kind: TagKind::Marker,
        doc: "Dissolved substance measurement (e.g. dissolved oxygen)",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "occupancyCount",
        kind: TagKind::Marker,
        doc: "Occupancy people count",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "peopleSensor",
        kind: TagKind::Marker,
        doc: "People counting sensor",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "motionSensor",
        kind: TagKind::Marker,
        doc: "Motion detection sensor",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    // ============================================================
    // Functional qualifiers
    // ============================================================
    TagDef {
        name: "hot",
        kind: TagKind::Marker,
        doc: "Hot fluid or loop",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "cold",
        kind: TagKind::Marker,
        doc: "Cold fluid or loop",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "chilled",
        kind: TagKind::Marker,
        doc: "Chilled fluid",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "condenser",
        kind: TagKind::Marker,
        doc: "Condenser loop",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "discharge",
        kind: TagKind::Marker,
        doc: "Discharge air/fluid",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "return",
        kind: TagKind::Marker,
        doc: "Return air/fluid",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "supply",
        kind: TagKind::Marker,
        doc: "Supply air/fluid",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "exhaust",
        kind: TagKind::Marker,
        doc: "Exhaust air/fluid",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "mixed",
        kind: TagKind::Marker,
        doc: "Mixed air/fluid",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "outside",
        kind: TagKind::Marker,
        doc: "Outside air/ambient",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "zone",
        kind: TagKind::Marker,
        doc: "Zone-level measurement",
        supertype: None,
        applies_to: &["point", "space"],
    },
    TagDef {
        name: "entering",
        kind: TagKind::Marker,
        doc: "Entering side of equipment",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "leaving",
        kind: TagKind::Marker,
        doc: "Leaving side of equipment",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "header",
        kind: TagKind::Marker,
        doc: "Header pipe/duct",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "bypass",
        kind: TagKind::Marker,
        doc: "Bypass path",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "delta",
        kind: TagKind::Marker,
        doc: "Difference/delta measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "effective",
        kind: TagKind::Marker,
        doc: "Effective/active value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "enable",
        kind: TagKind::Marker,
        doc: "Enable/disable control",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "run",
        kind: TagKind::Marker,
        doc: "Run status or command",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "alarm",
        kind: TagKind::Marker,
        doc: "Alarm status point",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "fault",
        kind: TagKind::Marker,
        doc: "Fault status point",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Equipment type markers
    // ============================================================
    TagDef {
        name: "ahu",
        kind: TagKind::Marker,
        doc: "Air Handling Unit",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "mau",
        kind: TagKind::Marker,
        doc: "Makeup Air Unit",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "rtu",
        kind: TagKind::Marker,
        doc: "Rooftop Unit",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "fcu",
        kind: TagKind::Marker,
        doc: "Fan Coil Unit",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "vav",
        kind: TagKind::Marker,
        doc: "Variable Air Volume box",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "boiler",
        kind: TagKind::Marker,
        doc: "Boiler",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "chiller",
        kind: TagKind::Marker,
        doc: "Chiller",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "coolingTower",
        kind: TagKind::Marker,
        doc: "Cooling tower",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "heatExchanger",
        kind: TagKind::Marker,
        doc: "Heat exchanger",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "pump",
        kind: TagKind::Marker,
        doc: "Pump",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "fan",
        kind: TagKind::Marker,
        doc: "Fan",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "damper",
        kind: TagKind::Marker,
        doc: "Damper",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "valve",
        kind: TagKind::Marker,
        doc: "Valve",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "meter",
        kind: TagKind::Marker,
        doc: "Meter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "panel",
        kind: TagKind::Marker,
        doc: "Electrical panel",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "ups",
        kind: TagKind::Marker,
        doc: "Uninterruptible Power Supply",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "vfd",
        kind: TagKind::Marker,
        doc: "Variable Frequency Drive",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "thermostat",
        kind: TagKind::Marker,
        doc: "Thermostat",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "heatPump",
        kind: TagKind::Marker,
        doc: "Heat pump",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "airTerminalUnit",
        kind: TagKind::Marker,
        doc: "Air terminal unit",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "coil",
        kind: TagKind::Marker,
        doc: "Heating or cooling coil",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "filter",
        kind: TagKind::Marker,
        doc: "Air or fluid filter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "humidifier",
        kind: TagKind::Marker,
        doc: "Humidifier",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "dehumidifier",
        kind: TagKind::Marker,
        doc: "Dehumidifier",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "tank",
        kind: TagKind::Marker,
        doc: "Storage tank",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "generator",
        kind: TagKind::Marker,
        doc: "Electrical generator",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    // ============================================================
    // Additional HVAC / piping equipment
    // ============================================================
    TagDef {
        name: "gasHeat",
        kind: TagKind::Marker,
        doc: "Gas-fired heating",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "oilHeat",
        kind: TagKind::Marker,
        doc: "Oil-fired heating",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "heatWheel",
        kind: TagKind::Marker,
        doc: "Rotary heat wheel energy recovery",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "heatRecovery",
        kind: TagKind::Marker,
        doc: "Heat recovery system",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "energyRecovery",
        kind: TagKind::Marker,
        doc: "Energy recovery ventilator",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "doas",
        kind: TagKind::Marker,
        doc: "Dedicated Outdoor Air System",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "crac",
        kind: TagKind::Marker,
        doc: "Computer Room Air Conditioner",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "crah",
        kind: TagKind::Marker,
        doc: "Computer Room Air Handler",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "unitHeater",
        kind: TagKind::Marker,
        doc: "Unit heater",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "radiantFloor",
        kind: TagKind::Marker,
        doc: "Radiant floor heating/cooling",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "radiantPanel",
        kind: TagKind::Marker,
        doc: "Radiant panel heating/cooling",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "baseboard",
        kind: TagKind::Marker,
        doc: "Baseboard heater",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "splitSystem",
        kind: TagKind::Marker,
        doc: "Split system HVAC",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "packagedUnit",
        kind: TagKind::Marker,
        doc: "Packaged HVAC unit",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "miniSplit",
        kind: TagKind::Marker,
        doc: "Mini-split ductless system",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "ductwork",
        kind: TagKind::Marker,
        doc: "Ductwork component",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "pipe",
        kind: TagKind::Marker,
        doc: "Pipe or piping segment",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "actuator",
        kind: TagKind::Marker,
        doc: "Actuator device",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    // ============================================================
    // Sub-equipment / component markers
    // ============================================================
    TagDef {
        name: "singleDuct",
        kind: TagKind::Marker,
        doc: "Single duct configuration",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "dualDuct",
        kind: TagKind::Marker,
        doc: "Dual duct configuration",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "multiZone",
        kind: TagKind::Marker,
        doc: "Multi-zone configuration",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "constantVolume",
        kind: TagKind::Marker,
        doc: "Constant volume airflow",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "variableVolume",
        kind: TagKind::Marker,
        doc: "Variable volume airflow",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "directExpansion",
        kind: TagKind::Marker,
        doc: "DX cooling",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "heating",
        kind: TagKind::Marker,
        doc: "Heating function",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "cooling",
        kind: TagKind::Marker,
        doc: "Cooling function",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "reheat",
        kind: TagKind::Marker,
        doc: "Reheat function",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "economizer",
        kind: TagKind::Marker,
        doc: "Economizer/free cooling",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    // ============================================================
    // Plant tags
    // ============================================================
    TagDef {
        name: "plant",
        kind: TagKind::Marker,
        doc: "Central plant",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "chilledWaterPlant",
        kind: TagKind::Marker,
        doc: "Chilled water plant",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "hotWaterPlant",
        kind: TagKind::Marker,
        doc: "Hot water plant",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "steamPlant",
        kind: TagKind::Marker,
        doc: "Steam generation plant",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "condenserWaterPlant",
        kind: TagKind::Marker,
        doc: "Condenser water plant",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "elecPanel",
        kind: TagKind::Marker,
        doc: "Electrical distribution panel",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    // ============================================================
    // Space type markers
    // ============================================================
    TagDef {
        name: "floor",
        kind: TagKind::Marker,
        doc: "Floor level within a building",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "room",
        kind: TagKind::Marker,
        doc: "Room or enclosed area",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "building",
        kind: TagKind::Marker,
        doc: "Building structure",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "wing",
        kind: TagKind::Marker,
        doc: "Wing of a building",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "roof",
        kind: TagKind::Marker,
        doc: "Rooftop area",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "mechanical",
        kind: TagKind::Marker,
        doc: "Mechanical room/area",
        supertype: None,
        applies_to: &["space"],
    },
    // ============================================================
    // Additional building / space types
    // ============================================================
    TagDef {
        name: "lobby",
        kind: TagKind::Marker,
        doc: "Lobby or reception area",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "corridor",
        kind: TagKind::Marker,
        doc: "Corridor or hallway",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "stairwell",
        kind: TagKind::Marker,
        doc: "Stairwell",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "elevator",
        kind: TagKind::Marker,
        doc: "Elevator or lift",
        supertype: None,
        applies_to: &["space", "equip"],
    },
    TagDef {
        name: "shaft",
        kind: TagKind::Marker,
        doc: "Vertical shaft (mechanical, elevator)",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "parking",
        kind: TagKind::Marker,
        doc: "Parking area or garage",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "exterior",
        kind: TagKind::Marker,
        doc: "Exterior or outdoor area",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "basement",
        kind: TagKind::Marker,
        doc: "Basement level",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "penthouse",
        kind: TagKind::Marker,
        doc: "Penthouse level or mechanical penthouse",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "mezzanine",
        kind: TagKind::Marker,
        doc: "Mezzanine level",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "openOffice",
        kind: TagKind::Marker,
        doc: "Open office area",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "privateOffice",
        kind: TagKind::Marker,
        doc: "Private office",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "conferenceRoom",
        kind: TagKind::Marker,
        doc: "Conference or meeting room",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "kitchen",
        kind: TagKind::Marker,
        doc: "Kitchen or break room",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "restroom",
        kind: TagKind::Marker,
        doc: "Restroom or washroom",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "dataCenter",
        kind: TagKind::Marker,
        doc: "Data center",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "serverRoom",
        kind: TagKind::Marker,
        doc: "Server room",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "idf",
        kind: TagKind::Marker,
        doc: "Intermediate Distribution Frame room",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "mdf",
        kind: TagKind::Marker,
        doc: "Main Distribution Frame room",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "cleanroom",
        kind: TagKind::Marker,
        doc: "Cleanroom environment",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "laboratory",
        kind: TagKind::Marker,
        doc: "Laboratory space",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    TagDef {
        name: "operatingRoom",
        kind: TagKind::Marker,
        doc: "Operating room / surgical suite",
        supertype: Some("space"),
        applies_to: &["space"],
    },
    // ============================================================
    // Relationship refs
    // ============================================================
    TagDef {
        name: "siteRef",
        kind: TagKind::Ref,
        doc: "Reference to parent site",
        supertype: None,
        applies_to: &["space", "equip", "point"],
    },
    TagDef {
        name: "equipRef",
        kind: TagKind::Ref,
        doc: "Reference to parent equipment",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "spaceRef",
        kind: TagKind::Ref,
        doc: "Reference to containing space",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "weatherStationRef",
        kind: TagKind::Ref,
        doc: "Reference to weather station",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "elecMeterRef",
        kind: TagKind::Ref,
        doc: "Reference to electric meter",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "hotWaterPlantRef",
        kind: TagKind::Ref,
        doc: "Reference to hot water plant",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "chilledWaterPlantRef",
        kind: TagKind::Ref,
        doc: "Reference to chilled water plant",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "steamPlantRef",
        kind: TagKind::Ref,
        doc: "Reference to steam plant",
        supertype: None,
        applies_to: &["equip"],
    },
    // ============================================================
    // Additional relationship refs
    // ============================================================
    TagDef {
        name: "ahuRef",
        kind: TagKind::Ref,
        doc: "Reference to parent AHU",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "vavRef",
        kind: TagKind::Ref,
        doc: "Reference to parent VAV",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "systemRef",
        kind: TagKind::Ref,
        doc: "Reference to parent system",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "meterRef",
        kind: TagKind::Ref,
        doc: "Reference to associated meter",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "panelRef",
        kind: TagKind::Ref,
        doc: "Reference to electrical panel",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "networkRef",
        kind: TagKind::Ref,
        doc: "Reference to network",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    // ============================================================
    // Status / capability tags
    // ============================================================
    TagDef {
        name: "cur",
        kind: TagKind::Marker,
        doc: "Has current real-time value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "his",
        kind: TagKind::Marker,
        doc: "Has historical time-series data",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "writable",
        kind: TagKind::Marker,
        doc: "Point value is writable",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "kind",
        kind: TagKind::Str,
        doc: "Point data kind (Bool, Number, Str)",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "unit",
        kind: TagKind::Str,
        doc: "Unit of measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "enum",
        kind: TagKind::Str,
        doc: "Enumeration range string",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "minVal",
        kind: TagKind::Number,
        doc: "Minimum value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "maxVal",
        kind: TagKind::Number,
        doc: "Maximum value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "curVal",
        kind: TagKind::Str,
        doc: "Current value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "curStatus",
        kind: TagKind::Str,
        doc: "Current value status",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "hisInterpolate",
        kind: TagKind::Str,
        doc: "History interpolation mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "hisInterval",
        kind: TagKind::Number,
        doc: "History collection interval",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Metadata
    // ============================================================
    TagDef {
        name: "dis",
        kind: TagKind::Str,
        doc: "Display name for the entity",
        supertype: None,
        applies_to: &["site", "space", "equip", "point"],
    },
    TagDef {
        name: "navName",
        kind: TagKind::Str,
        doc: "Navigation tree name",
        supertype: None,
        applies_to: &["site", "space", "equip", "point"],
    },
    TagDef {
        name: "tz",
        kind: TagKind::Str,
        doc: "IANA timezone identifier",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "area",
        kind: TagKind::Number,
        doc: "Area in square feet or meters",
        supertype: None,
        applies_to: &["site", "space"],
    },
    TagDef {
        name: "geoAddr",
        kind: TagKind::Str,
        doc: "Geographic street address",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "geoCoord",
        kind: TagKind::Coord,
        doc: "Geographic coordinates (lat/lng)",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "geoCity",
        kind: TagKind::Str,
        doc: "City name",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "geoState",
        kind: TagKind::Str,
        doc: "State or province",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "geoCountry",
        kind: TagKind::Str,
        doc: "Country code",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "geoPostalCode",
        kind: TagKind::Str,
        doc: "Postal/ZIP code",
        supertype: None,
        applies_to: &["site"],
    },
    TagDef {
        name: "primaryFunction",
        kind: TagKind::Str,
        doc: "Primary function of building",
        supertype: None,
        applies_to: &["site", "space"],
    },
    TagDef {
        name: "yearBuilt",
        kind: TagKind::Number,
        doc: "Year the building was constructed",
        supertype: None,
        applies_to: &["site"],
    },
    // ============================================================
    // Lighting
    // ============================================================
    TagDef {
        name: "light",
        kind: TagKind::Marker,
        doc: "Lighting system or point",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "lightLevel",
        kind: TagKind::Marker,
        doc: "Light level/illuminance",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Misc markers used in HVAC
    // ============================================================
    TagDef {
        name: "occ",
        kind: TagKind::Marker,
        doc: "Occupied mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "unocc",
        kind: TagKind::Marker,
        doc: "Unoccupied mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "standby",
        kind: TagKind::Marker,
        doc: "Standby mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "stage",
        kind: TagKind::Marker,
        doc: "Staging level",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "proof",
        kind: TagKind::Marker,
        doc: "Proof of operation",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "load",
        kind: TagKind::Marker,
        doc: "Load measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "efficiency",
        kind: TagKind::Marker,
        doc: "Efficiency measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "isolation",
        kind: TagKind::Marker,
        doc: "Isolation valve/damper",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "preheat",
        kind: TagKind::Marker,
        doc: "Preheat section",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "freezeStat",
        kind: TagKind::Marker,
        doc: "Freeze protection",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "filterDp",
        kind: TagKind::Marker,
        doc: "Filter differential pressure",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "ductPressure",
        kind: TagKind::Marker,
        doc: "Duct static pressure",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "ductArea",
        kind: TagKind::Marker,
        doc: "Duct cross-sectional area",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Electrical metering
    // ============================================================
    TagDef {
        name: "ac",
        kind: TagKind::Marker,
        doc: "Alternating current",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "dc",
        kind: TagKind::Marker,
        doc: "Direct current",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "phase",
        kind: TagKind::Str,
        doc: "Electrical phase (A, B, C)",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "total",
        kind: TagKind::Marker,
        doc: "Total/aggregate measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "avg",
        kind: TagKind::Marker,
        doc: "Average measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "reactive",
        kind: TagKind::Marker,
        doc: "Reactive power",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "apparent",
        kind: TagKind::Marker,
        doc: "Apparent power",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "pf",
        kind: TagKind::Marker,
        doc: "Power factor",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "demand",
        kind: TagKind::Marker,
        doc: "Demand measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "import",
        kind: TagKind::Marker,
        doc: "Imported energy/power",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "export",
        kind: TagKind::Marker,
        doc: "Exported energy/power",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "net",
        kind: TagKind::Marker,
        doc: "Net energy/power",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Weather
    // ============================================================
    TagDef {
        name: "weather",
        kind: TagKind::Marker,
        doc: "Weather-related point",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "weatherPoint",
        kind: TagKind::Marker,
        doc: "Weather station point",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "windSpeed",
        kind: TagKind::Marker,
        doc: "Wind speed",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "windDir",
        kind: TagKind::Marker,
        doc: "Wind direction",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "precipitation",
        kind: TagKind::Marker,
        doc: "Precipitation measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "solar",
        kind: TagKind::Marker,
        doc: "Solar irradiance",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "cloudage",
        kind: TagKind::Marker,
        doc: "Cloud coverage",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "dewPoint",
        kind: TagKind::Marker,
        doc: "Dew point temperature",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "wetBulb",
        kind: TagKind::Marker,
        doc: "Wet bulb temperature",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "dryBulb",
        kind: TagKind::Marker,
        doc: "Dry bulb temperature",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "enthalpy",
        kind: TagKind::Marker,
        doc: "Enthalpy",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "visibility",
        kind: TagKind::Marker,
        doc: "Visibility distance",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "daytime",
        kind: TagKind::Marker,
        doc: "Daytime flag",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "barometric",
        kind: TagKind::Marker,
        doc: "Barometric pressure",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Process / control tags
    // ============================================================
    TagDef {
        name: "pid",
        kind: TagKind::Marker,
        doc: "PID control loop",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "loop",
        kind: TagKind::Marker,
        doc: "Control loop",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "deadband",
        kind: TagKind::Marker,
        doc: "Deadband value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "reset",
        kind: TagKind::Marker,
        doc: "Reset control strategy",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "cascade",
        kind: TagKind::Marker,
        doc: "Cascade control",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "lead",
        kind: TagKind::Marker,
        doc: "Lead equipment in lead/lag sequence",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "lag",
        kind: TagKind::Marker,
        doc: "Lag equipment in lead/lag sequence",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "alternate",
        kind: TagKind::Marker,
        doc: "Alternating duty equipment",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "hand",
        kind: TagKind::Marker,
        doc: "Hand/manual mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "auto",
        kind: TagKind::Marker,
        doc: "Automatic mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "override",
        kind: TagKind::Marker,
        doc: "Override mode or value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "lockout",
        kind: TagKind::Marker,
        doc: "Lockout condition",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "interlock",
        kind: TagKind::Marker,
        doc: "Safety interlock",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "safetyCircuit",
        kind: TagKind::Marker,
        doc: "Safety circuit status",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "fireAlarm",
        kind: TagKind::Marker,
        doc: "Fire alarm point",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "fireSmokeControl",
        kind: TagKind::Marker,
        doc: "Fire smoke control mode",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "economizing",
        kind: TagKind::Marker,
        doc: "Economizer active mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "dehumidifying",
        kind: TagKind::Marker,
        doc: "Dehumidification active mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "humidifying",
        kind: TagKind::Marker,
        doc: "Humidification active mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "warmup",
        kind: TagKind::Marker,
        doc: "Warmup mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "cooldown",
        kind: TagKind::Marker,
        doc: "Cooldown mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "nightSetback",
        kind: TagKind::Marker,
        doc: "Night setback mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "morningWarmup",
        kind: TagKind::Marker,
        doc: "Morning warmup mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "optimumStart",
        kind: TagKind::Marker,
        doc: "Optimum start control",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Energy / sustainability
    // ============================================================
    TagDef {
        name: "photovoltaic",
        kind: TagKind::Marker,
        doc: "Photovoltaic solar panel",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "inverter",
        kind: TagKind::Marker,
        doc: "DC to AC inverter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "battery",
        kind: TagKind::Marker,
        doc: "Battery energy storage",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "charger",
        kind: TagKind::Marker,
        doc: "Battery charger or EV charger",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "solarThermal",
        kind: TagKind::Marker,
        doc: "Solar thermal collector",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "geothermal",
        kind: TagKind::Marker,
        doc: "Geothermal system",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "windTurbine",
        kind: TagKind::Marker,
        doc: "Wind turbine generator",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "peakDemand",
        kind: TagKind::Marker,
        doc: "Peak demand measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "powerFactor",
        kind: TagKind::Marker,
        doc: "Power factor measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "carbonFootprint",
        kind: TagKind::Marker,
        doc: "Carbon footprint measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "ghg",
        kind: TagKind::Marker,
        doc: "Greenhouse gas emissions",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Water / plumbing
    // ============================================================
    TagDef {
        name: "domesticWater",
        kind: TagKind::Marker,
        doc: "Domestic hot/cold water system",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "stormWater",
        kind: TagKind::Marker,
        doc: "Storm water system",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "greyWater",
        kind: TagKind::Marker,
        doc: "Grey water reclamation system",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "irrigation",
        kind: TagKind::Marker,
        doc: "Irrigation system",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "rainwater",
        kind: TagKind::Marker,
        doc: "Rainwater collection system",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "sewage",
        kind: TagKind::Marker,
        doc: "Sewage or wastewater system",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "well",
        kind: TagKind::Marker,
        doc: "Water well",
        supertype: None,
        applies_to: &["equip"],
    },
    // ============================================================
    // Sensor types (field instrument classification)
    // ============================================================
    TagDef {
        name: "thermocouple",
        kind: TagKind::Marker,
        doc: "Thermocouple temperature sensor",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "rtd",
        kind: TagKind::Marker,
        doc: "RTD temperature sensor",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "thermistor",
        kind: TagKind::Marker,
        doc: "Thermistor temperature sensor",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "transducer",
        kind: TagKind::Marker,
        doc: "Transducer device",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "transmitter",
        kind: TagKind::Marker,
        doc: "Transmitter device",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    // ============================================================
    // Network / protocol tags
    // ============================================================
    TagDef {
        name: "bacnet",
        kind: TagKind::Marker,
        doc: "BACnet protocol",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "modbus",
        kind: TagKind::Marker,
        doc: "Modbus protocol",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "lonworks",
        kind: TagKind::Marker,
        doc: "LonWorks protocol",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "knx",
        kind: TagKind::Marker,
        doc: "KNX protocol",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "mqtt",
        kind: TagKind::Marker,
        doc: "MQTT protocol",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "opcua",
        kind: TagKind::Marker,
        doc: "OPC-UA protocol",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "ipAddr",
        kind: TagKind::Str,
        doc: "IP address",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "port",
        kind: TagKind::Number,
        doc: "Network port number",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "deviceId",
        kind: TagKind::Str,
        doc: "Protocol device identifier",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "objectId",
        kind: TagKind::Str,
        doc: "Protocol object identifier",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Commissioning / maintenance
    // ============================================================
    TagDef {
        name: "commissioned",
        kind: TagKind::Marker,
        doc: "Equipment has been commissioned",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "tested",
        kind: TagKind::Marker,
        doc: "Equipment has been tested",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "calibrated",
        kind: TagKind::Marker,
        doc: "Sensor/instrument has been calibrated",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "maintained",
        kind: TagKind::Marker,
        doc: "Equipment is under maintenance",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "warrantyStart",
        kind: TagKind::Date,
        doc: "Warranty start date",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "warrantyEnd",
        kind: TagKind::Date,
        doc: "Warranty end date",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "installedDate",
        kind: TagKind::Date,
        doc: "Installation date",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "manufacturer",
        kind: TagKind::Str,
        doc: "Manufacturer name",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "model",
        kind: TagKind::Str,
        doc: "Model number or name",
        supertype: None,
        applies_to: &["equip"],
    },
    TagDef {
        name: "serialNum",
        kind: TagKind::Str,
        doc: "Serial number",
        supertype: None,
        applies_to: &["equip"],
    },
    // ============================================================
    // Additional Haystack 4 core entity tags
    // ============================================================
    TagDef {
        name: "id",
        kind: TagKind::Ref,
        doc: "Unique identifier for entity",
        supertype: None,
        applies_to: &["site", "space", "equip", "point"],
    },
    TagDef {
        name: "mod",
        kind: TagKind::DateTime,
        doc: "Last modified timestamp",
        supertype: None,
        applies_to: &["site", "space", "equip", "point"],
    },
    TagDef {
        name: "trash",
        kind: TagKind::Marker,
        doc: "Entity has been soft-deleted",
        supertype: None,
        applies_to: &["site", "space", "equip", "point"],
    },
    // ============================================================
    // Additional point / data tags
    // ============================================================
    TagDef {
        name: "hisEnd",
        kind: TagKind::DateTime,
        doc: "End timestamp of history data",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "hisSize",
        kind: TagKind::Number,
        doc: "Approximate size of history data",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "hisTotalized",
        kind: TagKind::Marker,
        doc: "History is totalized (cumulative)",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "curErr",
        kind: TagKind::Str,
        doc: "Current value error message",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "writeVal",
        kind: TagKind::Str,
        doc: "Current write value",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "writeLevel",
        kind: TagKind::Number,
        doc: "Current write priority level",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "writeStatus",
        kind: TagKind::Str,
        doc: "Current write status",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Additional functional qualifiers
    // ============================================================
    TagDef {
        name: "inlet",
        kind: TagKind::Marker,
        doc: "Inlet side",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "outlet",
        kind: TagKind::Marker,
        doc: "Outlet side",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "primary",
        kind: TagKind::Marker,
        doc: "Primary loop or circuit",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "secondary",
        kind: TagKind::Marker,
        doc: "Secondary loop or circuit",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "tertiary",
        kind: TagKind::Marker,
        doc: "Tertiary loop or circuit",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "min",
        kind: TagKind::Marker,
        doc: "Minimum value qualifier",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "max",
        kind: TagKind::Marker,
        doc: "Maximum value qualifier",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Additional HVAC operational modes
    // ============================================================
    TagDef {
        name: "ventilation",
        kind: TagKind::Marker,
        doc: "Ventilation mode or system",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "recirculating",
        kind: TagKind::Marker,
        doc: "Recirculating air mode",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "directZone",
        kind: TagKind::Marker,
        doc: "Direct zone control",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "faceBypass",
        kind: TagKind::Marker,
        doc: "Face and bypass damper control",
        supertype: None,
        applies_to: &["point", "equip"],
    },
    TagDef {
        name: "freezeProtect",
        kind: TagKind::Marker,
        doc: "Freeze protection mode",
        supertype: None,
        applies_to: &["point"],
    },
    // ============================================================
    // Additional equipment subtypes
    // ============================================================
    TagDef {
        name: "compressor",
        kind: TagKind::Marker,
        doc: "Compressor equipment",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "condenserFan",
        kind: TagKind::Marker,
        doc: "Condenser fan",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "evaporator",
        kind: TagKind::Marker,
        doc: "Evaporator coil or unit",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "furnace",
        kind: TagKind::Marker,
        doc: "Furnace",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "airCurtain",
        kind: TagKind::Marker,
        doc: "Air curtain",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "exhaustFan",
        kind: TagKind::Marker,
        doc: "Exhaust fan",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "supplyFan",
        kind: TagKind::Marker,
        doc: "Supply fan",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "returnFan",
        kind: TagKind::Marker,
        doc: "Return fan",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "reliefFan",
        kind: TagKind::Marker,
        doc: "Relief fan",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "transferFan",
        kind: TagKind::Marker,
        doc: "Transfer fan",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "circPump",
        kind: TagKind::Marker,
        doc: "Circulation pump",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "condenserPump",
        kind: TagKind::Marker,
        doc: "Condenser water pump",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "boosterPump",
        kind: TagKind::Marker,
        doc: "Booster pump",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "sumpPump",
        kind: TagKind::Marker,
        doc: "Sump pump",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "waterHeater",
        kind: TagKind::Marker,
        doc: "Domestic water heater",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "coolingCoil",
        kind: TagKind::Marker,
        doc: "Cooling coil",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "heatingCoil",
        kind: TagKind::Marker,
        doc: "Heating coil",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "mixingBox",
        kind: TagKind::Marker,
        doc: "Mixing box or plenum",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "outdoorAirDamper",
        kind: TagKind::Marker,
        doc: "Outdoor air intake damper",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "returnAirDamper",
        kind: TagKind::Marker,
        doc: "Return air damper",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "exhaustAirDamper",
        kind: TagKind::Marker,
        doc: "Exhaust air damper",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "fireDamper",
        kind: TagKind::Marker,
        doc: "Fire damper",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "smokeDamper",
        kind: TagKind::Marker,
        doc: "Smoke damper",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "checkValve",
        kind: TagKind::Marker,
        doc: "Check valve",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "balancingValve",
        kind: TagKind::Marker,
        doc: "Balancing valve",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "pressureReliefValve",
        kind: TagKind::Marker,
        doc: "Pressure relief valve",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "flowSwitch",
        kind: TagKind::Marker,
        doc: "Flow switch",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "pressureSwitch",
        kind: TagKind::Marker,
        doc: "Pressure switch",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "tempSwitch",
        kind: TagKind::Marker,
        doc: "Temperature switch",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    // ============================================================
    // Additional meter types
    // ============================================================
    TagDef {
        name: "elecMeter",
        kind: TagKind::Marker,
        doc: "Electrical meter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "gasMeter",
        kind: TagKind::Marker,
        doc: "Gas meter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "waterMeter",
        kind: TagKind::Marker,
        doc: "Water meter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "steamMeter",
        kind: TagKind::Marker,
        doc: "Steam meter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "btuMeter",
        kind: TagKind::Marker,
        doc: "BTU / thermal energy meter",
        supertype: Some("equip"),
        applies_to: &["equip"],
    },
    TagDef {
        name: "subMeter",
        kind: TagKind::Marker,
        doc: "Sub-meter",
        supertype: None,
        applies_to: &["equip"],
    },
    // ============================================================
    // Additional electrical tags
    // ============================================================
    TagDef {
        name: "threePhase",
        kind: TagKind::Marker,
        doc: "Three-phase electrical",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "singlePhase",
        kind: TagKind::Marker,
        doc: "Single-phase electrical",
        supertype: None,
        applies_to: &["equip", "point"],
    },
    TagDef {
        name: "lineToLine",
        kind: TagKind::Marker,
        doc: "Line-to-line measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "lineToNeutral",
        kind: TagKind::Marker,
        doc: "Line-to-neutral measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "phaseA",
        kind: TagKind::Marker,
        doc: "Phase A",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "phaseB",
        kind: TagKind::Marker,
        doc: "Phase B",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "phaseC",
        kind: TagKind::Marker,
        doc: "Phase C",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "neutral",
        kind: TagKind::Marker,
        doc: "Neutral conductor",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "ground",
        kind: TagKind::Marker,
        doc: "Ground / earth conductor",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "harmonic",
        kind: TagKind::Marker,
        doc: "Harmonic distortion measurement",
        supertype: None,
        applies_to: &["point"],
    },
    TagDef {
        name: "thd",
        kind: TagKind::Marker,
        doc: "Total harmonic distortion",
        supertype: None,
        applies_to: &["point"],
    },
];

/// Haystack 4 standard units, grouped by quantity.
pub static UNITS: &[(&str, &[&str])] = &[
    ("temperature", &["°F", "°C", "K"]),
    (
        "pressure",
        &[
            "psi", "kPa", "inH₂O", "Pa", "bar", "mbar", "inHg", "mmHg", "atm",
        ],
    ),
    ("flow", &["cfm", "L/s", "m³/s", "m³/h", "gpm", "L/min"]),
    ("volumetric_flow", &["m³/min"]),
    ("mass_flow", &["kg/s", "lb/h", "kg/h"]),
    ("area", &["ft²", "m²"]),
    ("volume", &["ft³", "m³", "gal", "L"]),
    (
        "energy",
        &["kWh", "MWh", "BTU", "kBTU", "MBTU", "GJ", "MJ", "therm"],
    ),
    ("enthalpy", &["BTU/lb", "kJ/kg"]),
    ("power", &["kW", "MW", "W", "BTU/h", "hp", "ton"]),
    ("frequency", &["Hz", "kHz", "MHz", "rpm"]),
    ("voltage", &["V", "kV", "mV"]),
    ("current", &["A", "mA", "kA"]),
    ("speed", &["ft/min", "m/s", "mph", "km/h"]),
    ("humidity", &["%RH"]),
    ("concentration", &["ppm", "ppb", "mg/m³", "µg/m³"]),
    ("illuminance", &["lux", "fc"]),
    ("luminous_flux", &["lm"]),
    ("luminous_intensity", &["cd"]),
    ("time", &["s", "min", "h", "day"]),
    ("length", &["ft", "m", "in", "cm", "mm"]),
    ("mass", &["lb", "kg", "ton_metric"]),
    ("percent", &["%"]),
    ("angle", &["deg", "rad"]),
    ("resistance", &["Ω", "kΩ"]),
    ("reactive_power", &["kVAR", "VAR"]),
    ("apparent_power", &["kVA", "VA"]),
    ("power_factor", &["pf"]),
    ("data", &["byte", "KB", "MB", "GB", "TB"]),
    ("currency", &["USD", "EUR", "GBP"]),
    ("co2_emission", &["kg_CO2", "ton_CO2"]),
    ("density", &["kg/m³", "lb/ft³"]),
    ("torque", &["Nm", "ft·lb"]),
];

// ----------------------------------------------------------------
// Lookup functions
// ----------------------------------------------------------------

/// Look up a tag by name (case-sensitive).
pub fn find_tag(name: &str) -> Option<&'static TagDef> {
    TAGS.iter().find(|t| t.name == name)
}

/// Return all tags relevant to a given entity type.
pub fn tags_for_entity(entity_type: &str) -> Vec<&'static TagDef> {
    TAGS.iter()
        .filter(|t| t.applies_to.contains(&entity_type))
        .collect()
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_known_tags() {
        assert!(find_tag("site").is_some());
        assert!(find_tag("ahu").is_some());
        assert!(find_tag("temp").is_some());
        assert!(find_tag("siteRef").is_some());
        assert!(find_tag("nonexistent").is_none());
    }

    #[test]
    fn tags_for_site() {
        let site_tags = tags_for_entity("site");
        let names: Vec<&str> = site_tags.iter().map(|t| t.name).collect();
        assert!(names.contains(&"site"));
        assert!(names.contains(&"dis"));
        assert!(names.contains(&"tz"));
        assert!(!names.contains(&"sensor"));
    }

    #[test]
    fn tags_for_point() {
        let point_tags = tags_for_entity("point");
        let names: Vec<&str> = point_tags.iter().map(|t| t.name).collect();
        assert!(names.contains(&"point"));
        assert!(names.contains(&"sensor"));
        assert!(names.contains(&"cmd"));
        assert!(names.contains(&"sp"));
        assert!(names.contains(&"temp"));
        assert!(names.contains(&"unit"));
        assert!(!names.contains(&"site"));
    }

    #[test]
    fn tags_for_equip() {
        let equip_tags = tags_for_entity("equip");
        let names: Vec<&str> = equip_tags.iter().map(|t| t.name).collect();
        assert!(names.contains(&"equip"));
        assert!(names.contains(&"ahu"));
        assert!(names.contains(&"vav"));
        assert!(names.contains(&"pump"));
        assert!(names.contains(&"equipRef"));
    }

    #[test]
    fn no_duplicate_tag_names() {
        let mut names: Vec<&str> = TAGS.iter().map(|t| t.name).collect();
        names.sort();
        let len_before = names.len();
        names.dedup();
        assert_eq!(len_before, names.len(), "duplicate tag names found");
    }

    #[test]
    fn units_not_empty() {
        assert!(UNITS.len() > 10);
        for &(quantity, units) in UNITS {
            assert!(!quantity.is_empty());
            assert!(!units.is_empty());
        }
    }

    #[test]
    fn has_new_category_tags() {
        // Plant tags
        assert!(find_tag("plant").is_some());
        assert!(find_tag("chilledWaterPlant").is_some());
        assert!(find_tag("hotWaterPlant").is_some());
        assert!(find_tag("steamPlant").is_some());
        assert!(find_tag("condenserWaterPlant").is_some());

        // Process/control tags
        assert!(find_tag("pid").is_some());
        assert!(find_tag("deadband").is_some());
        assert!(find_tag("lockout").is_some());
        assert!(find_tag("optimumStart").is_some());

        // Energy/sustainability
        assert!(find_tag("photovoltaic").is_some());
        assert!(find_tag("battery").is_some());
        assert!(find_tag("ghg").is_some());

        // Water/plumbing
        assert!(find_tag("domesticWater").is_some());
        assert!(find_tag("stormWater").is_some());
        assert!(find_tag("sewage").is_some());

        // Network/protocol
        assert!(find_tag("bacnet").is_some());
        assert!(find_tag("modbus").is_some());
        assert!(find_tag("mqtt").is_some());

        // Additional refs
        assert!(find_tag("ahuRef").is_some());
        assert!(find_tag("vavRef").is_some());
        assert!(find_tag("networkRef").is_some());

        // Space types
        assert!(find_tag("dataCenter").is_some());
        assert!(find_tag("laboratory").is_some());
        assert!(find_tag("conferenceRoom").is_some());

        // Commissioning
        assert!(find_tag("manufacturer").is_some());
        assert!(find_tag("serialNum").is_some());
        assert!(find_tag("commissioned").is_some());

        // Additional HVAC
        assert!(find_tag("crac").is_some());
        assert!(find_tag("doas").is_some());
        assert!(find_tag("miniSplit").is_some());

        // Sensor types
        assert!(find_tag("thermocouple").is_some());
        assert!(find_tag("rtd").is_some());

        // Additional measurements
        assert!(find_tag("vibration").is_some());
        assert!(find_tag("particulate").is_some());
        assert!(find_tag("phLevel").is_some());
    }

    #[test]
    fn tag_count_comprehensive() {
        // We should have well over 250 tags now
        assert!(
            TAGS.len() >= 250,
            "Expected >= 250 tags, got {}",
            TAGS.len()
        );
    }
}
