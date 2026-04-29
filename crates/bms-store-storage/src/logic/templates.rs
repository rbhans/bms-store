//! Pre-built program templates for common BAS control sequences.
//!
//! Each template creates a [`Program`] with blocks, wires, and reasonable
//! defaults.  Point-read and point-write `node_id` fields are set to
//! descriptive placeholder strings (e.g. `"outdoor_air_temp"`) — the user
//! should rebind them to actual node IDs after instantiation.

use crate::config::profile::PointValue;
use crate::logic::model::*;

// ── Template metadata ──────────────────────────────────────────────

/// Category for grouping templates in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    Hvac,
    Energy,
    Safety,
    Lighting,
    Utility,
}

impl TemplateCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hvac => "HVAC",
            Self::Energy => "Energy Management",
            Self::Safety => "Safety & Alarms",
            Self::Lighting => "Lighting",
            Self::Utility => "General Utilities",
        }
    }

    pub fn all() -> &'static [TemplateCategory] {
        &[
            Self::Hvac,
            Self::Energy,
            Self::Safety,
            Self::Lighting,
            Self::Utility,
        ]
    }
}

/// Description of a template shown in the UI before instantiation.
pub struct TemplateInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub category: TemplateCategory,
    pub description: &'static str,
}

/// Return the catalog of all available templates.
pub fn catalog() -> Vec<TemplateInfo> {
    vec![
        TemplateInfo {
            id: "economizer",
            name: "AHU Economizer",
            category: TemplateCategory::Hvac,
            description: "Outdoor-air free cooling with high-limit lockout and PID-modulated damper.",
        },
        TemplateInfo {
            id: "dat_control",
            name: "Discharge Air Temp Control",
            category: TemplateCategory::Hvac,
            description: "Heating/cooling valve sequence from a single PID loop (0–200 split range).",
        },
        TemplateInfo {
            id: "fan_staging",
            name: "Fan Staging (2-Stage)",
            category: TemplateCategory::Hvac,
            description: "Stage supply fans on/off based on duct static pressure with anti-short-cycle delays.",
        },
        TemplateInfo {
            id: "vav_control",
            name: "VAV Box Control",
            category: TemplateCategory::Hvac,
            description: "Zone temperature control with variable damper and reheat valve, minimum airflow enforced.",
        },
        TemplateInfo {
            id: "chiller_staging",
            name: "Chiller Staging (3-Stage)",
            category: TemplateCategory::Hvac,
            description: "Stage up to three chillers based on load with anti-short-cycle timers.",
        },
        TemplateInfo {
            id: "boiler_oat_reset",
            name: "Boiler OAT Reset",
            category: TemplateCategory::Hvac,
            description: "Reset hot water supply setpoint from outdoor air temperature with ramp limiting.",
        },
        TemplateInfo {
            id: "changeover",
            name: "Heating/Cooling Changeover",
            category: TemplateCategory::Hvac,
            description: "Automatic 2-pipe system mode switch based on smoothed outdoor temperature.",
        },
        TemplateInfo {
            id: "demand_limiting",
            name: "Demand Limiting",
            category: TemplateCategory::Energy,
            description: "Shed loads when building power approaches utility demand limit.",
        },
        TemplateInfo {
            id: "optimal_start",
            name: "Optimal Start",
            category: TemplateCategory::Energy,
            description: "Start HVAC early enough to reach setpoint exactly at occupancy time.",
        },
        TemplateInfo {
            id: "night_setback",
            name: "Night Setback / Unoccupied",
            category: TemplateCategory::Energy,
            description: "Widen temperature deadband during unoccupied hours with tenant override.",
        },
        TemplateInfo {
            id: "freeze_protect",
            name: "Freeze Protection",
            category: TemplateCategory::Safety,
            description: "Open heating valve and close OA damper when low air temperature detected.",
        },
        TemplateInfo {
            id: "high_low_limit",
            name: "High/Low Limit Monitor",
            category: TemplateCategory::Safety,
            description: "Monitor any analog point against configurable limits with deadband and delay.",
        },
        TemplateInfo {
            id: "runtime_monitor",
            name: "Equipment Runtime Monitor",
            category: TemplateCategory::Safety,
            description: "Track run hours and alarm on maintenance interval or excessive continuous runtime.",
        },
        TemplateInfo {
            id: "filter_dp",
            name: "Filter DP Monitor",
            category: TemplateCategory::Safety,
            description: "Monitor filter differential pressure and alarm when filter is dirty.",
        },
        TemplateInfo {
            id: "proof_of_operation",
            name: "Proof of Operation",
            category: TemplateCategory::Safety,
            description: "Verify equipment started after command; alarm if no proof within timeout.",
        },
        TemplateInfo {
            id: "occ_lighting",
            name: "Occupancy Lighting",
            category: TemplateCategory::Lighting,
            description: "Control lights based on occupancy sensor with vacancy delay and manual override.",
        },
        TemplateInfo {
            id: "daylight_harvest",
            name: "Daylight Harvesting",
            category: TemplateCategory::Lighting,
            description: "Dim artificial lights based on photosensor to maintain target illuminance.",
        },
        TemplateInfo {
            id: "lead_lag",
            name: "Lead/Lag Rotation",
            category: TemplateCategory::Utility,
            description: "Alternate lead/lag equipment to equalize runtime; auto-switch on fault.",
        },
        TemplateInfo {
            id: "duct_static_reset",
            name: "Duct Static Pressure Reset",
            category: TemplateCategory::Utility,
            description: "Reset duct pressure setpoint down when VAV dampers are not requesting.",
        },
        TemplateInfo {
            id: "cascade_pid",
            name: "Cascade PID Control",
            category: TemplateCategory::Utility,
            description: "Outer-loop PID sets the setpoint for an inner-loop PID (e.g. zone→discharge).",
        },
    ]
}

// ── Helpers ────────────────────────────────────────────────────────

fn blk(id: &str, bt: BlockType, x: f64, y: f64) -> Block {
    Block {
        id: id.into(),
        block_type: bt,
        x,
        y,
        enabled: true,
    }
}

fn wire(from_block: &str, from_port: &str, to_block: &str, to_port: &str) -> Wire {
    Wire {
        from_block: from_block.into(),
        from_port: from_port.into(),
        to_block: to_block.into(),
        to_port: to_port.into(),
    }
}

fn read(id: &str, node: &str, x: f64, y: f64) -> Block {
    blk(
        id,
        BlockType::PointRead {
            node_id: node.into(),
        },
        x,
        y,
    )
}

fn write_pt(id: &str, node: &str, prio: Option<u8>, x: f64, y: f64) -> Block {
    blk(
        id,
        BlockType::PointWrite {
            node_id: node.into(),
            priority: prio,
        },
        x,
        y,
    )
}

fn constant(id: &str, val: f64, x: f64, y: f64) -> Block {
    blk(
        id,
        BlockType::Constant {
            value: PointValue::Float(val),
        },
        x,
        y,
    )
}

fn alarm(id: &str, node: &str, msg: &str, x: f64, y: f64) -> Block {
    blk(
        id,
        BlockType::AlarmTrigger {
            node_id: node.into(),
            message: msg.into(),
        },
        x,
        y,
    )
}

fn program(
    id: &str,
    name: &str,
    desc: &str,
    interval_ms: u64,
    blocks: Vec<Block>,
    wires: Vec<Wire>,
) -> Program {
    Program {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        enabled: false,
        trigger: Trigger::Periodic { interval_ms },
        blocks,
        wires,
        rhai_override: None,
        created_ms: 0,
        updated_ms: 0,
    }
}

// ── Instantiate ────────────────────────────────────────────────────

/// Create a program from a template ID.  Returns `None` if the ID is unknown.
pub fn instantiate(template_id: &str) -> Option<Program> {
    match template_id {
        "economizer" => Some(economizer()),
        "dat_control" => Some(dat_control()),
        "fan_staging" => Some(fan_staging()),
        "vav_control" => Some(vav_control()),
        "chiller_staging" => Some(chiller_staging()),
        "boiler_oat_reset" => Some(boiler_oat_reset()),
        "changeover" => Some(changeover()),
        "demand_limiting" => Some(demand_limiting()),
        "optimal_start" => Some(optimal_start()),
        "night_setback" => Some(night_setback()),
        "freeze_protect" => Some(freeze_protect()),
        "high_low_limit" => Some(high_low_limit()),
        "runtime_monitor" => Some(runtime_monitor()),
        "filter_dp" => Some(filter_dp()),
        "proof_of_operation" => Some(proof_of_operation()),
        "occ_lighting" => Some(occ_lighting()),
        "daylight_harvest" => Some(daylight_harvest()),
        "lead_lag" => Some(lead_lag()),
        "duct_static_reset" => Some(duct_static_reset()),
        "cascade_pid" => Some(cascade_pid()),
        _ => None,
    }
}

// ── HVAC Templates ─────────────────────────────────────────────────

fn economizer() -> Program {
    let blocks = vec![
        read("oat", "outdoor_air_temp", 60.0, 60.0),
        read("rat", "return_air_temp", 60.0, 180.0),
        constant("hi_limit", 65.0, 60.0, 300.0),
        constant("min_pos", 15.0, 60.0, 420.0),
        // OAT < high limit?
        blk(
            "cmp_hi",
            BlockType::Compare { op: CompareOp::Lt },
            260.0,
            100.0,
        ),
        // OAT < RAT?
        blk(
            "cmp_rat",
            BlockType::Compare { op: CompareOp::Lt },
            260.0,
            220.0,
        ),
        // Both conditions → economizer enabled
        blk(
            "and_eco",
            BlockType::Logic { op: LogicOp::And },
            460.0,
            160.0,
        ),
        // PID: DAT → damper position
        read("dat", "discharge_air_temp", 260.0, 380.0),
        read("dat_sp", "discharge_air_temp_sp", 260.0, 480.0),
        blk(
            "pid_damp",
            BlockType::Pid {
                kp: 2.0,
                ki: 0.1,
                kd: 0.0,
                output_min: 0.0,
                output_max: 100.0,
            },
            460.0,
            420.0,
        ),
        // Select: eco enabled → PID, else → min damper
        blk("sel", BlockType::Select, 660.0, 280.0),
        write_pt("w_damp", "outdoor_air_damper_cmd", Some(8), 860.0, 280.0),
    ];
    let wires = vec![
        wire("oat", "value", "cmp_hi", "a"),
        wire("hi_limit", "value", "cmp_hi", "b"),
        wire("oat", "value", "cmp_rat", "a"),
        wire("rat", "value", "cmp_rat", "b"),
        wire("cmp_hi", "result", "and_eco", "a"),
        wire("cmp_rat", "result", "and_eco", "b"),
        wire("dat", "value", "pid_damp", "process_variable"),
        wire("dat_sp", "value", "pid_damp", "setpoint"),
        wire("and_eco", "result", "sel", "condition"),
        wire("pid_damp", "output", "sel", "if_true"),
        wire("min_pos", "value", "sel", "if_false"),
        wire("sel", "result", "w_damp", "value"),
    ];
    program(
        "economizer",
        "AHU Economizer",
        "Outdoor-air free cooling with high-limit lockout and PID-modulated damper.",
        5000,
        blocks,
        wires,
    )
}

fn dat_control() -> Program {
    let blocks = vec![
        read("dat", "discharge_air_temp", 60.0, 120.0),
        read("dat_sp", "discharge_air_temp_sp", 60.0, 240.0),
        blk(
            "pid",
            BlockType::Pid {
                kp: 2.0,
                ki: 0.1,
                kd: 0.0,
                output_min: 0.0,
                output_max: 200.0,
            },
            300.0,
            180.0,
        ),
        constant("c100", 100.0, 300.0, 320.0),
        // Heating = clamp(PID, 0, 100)
        blk(
            "clamp_htg",
            BlockType::Math { op: MathOp::Clamp },
            540.0,
            100.0,
        ),
        constant("c0_htg", 0.0, 340.0, 40.0),
        constant("c100_htg", 100.0, 340.0, 160.0),
        // Cooling = clamp(PID - 100, 0, 100)
        blk("sub_100", BlockType::Math { op: MathOp::Sub }, 540.0, 280.0),
        blk(
            "clamp_clg",
            BlockType::Math { op: MathOp::Clamp },
            740.0,
            280.0,
        ),
        constant("c0_clg", 0.0, 540.0, 380.0),
        constant("c100_clg", 100.0, 540.0, 440.0),
        write_pt("w_htg", "heating_valve_cmd", Some(8), 780.0, 100.0),
        write_pt("w_clg", "cooling_valve_cmd", Some(8), 940.0, 280.0),
    ];
    let wires = vec![
        wire("dat", "value", "pid", "process_variable"),
        wire("dat_sp", "value", "pid", "setpoint"),
        // Heating clamp
        wire("pid", "output", "clamp_htg", "value"),
        wire("c0_htg", "value", "clamp_htg", "min"),
        wire("c100_htg", "value", "clamp_htg", "max"),
        wire("clamp_htg", "result", "w_htg", "value"),
        // Cooling: PID - 100
        wire("pid", "output", "sub_100", "a"),
        wire("c100", "value", "sub_100", "b"),
        wire("sub_100", "result", "clamp_clg", "value"),
        wire("c0_clg", "value", "clamp_clg", "min"),
        wire("c100_clg", "value", "clamp_clg", "max"),
        wire("clamp_clg", "result", "w_clg", "value"),
    ];
    program(
        "dat_control",
        "Discharge Air Temp Control",
        "Heating/cooling valve sequence from a single PID loop (0–200 split range).",
        5000,
        blocks,
        wires,
    )
}

fn fan_staging() -> Program {
    let blocks = vec![
        read("sp", "duct_static_pressure", 60.0, 100.0),
        constant("setpt", 1.5, 60.0, 220.0),
        constant("up_off", -0.3, 60.0, 340.0),
        constant("dn_off", 0.2, 60.0, 440.0),
        // stage-up threshold = SP + offset
        blk("add_up", BlockType::Math { op: MathOp::Add }, 260.0, 260.0),
        // stage-down threshold = SP + offset
        blk("add_dn", BlockType::Math { op: MathOp::Add }, 260.0, 400.0),
        // pressure < stage-up → need more fan
        blk(
            "cmp_up",
            BlockType::Compare { op: CompareOp::Lt },
            460.0,
            140.0,
        ),
        // pressure > stage-down → can shed
        blk(
            "cmp_dn",
            BlockType::Compare { op: CompareOp::Gt },
            460.0,
            340.0,
        ),
        // Delay before staging (prevent short cycling)
        blk(
            "dly_up",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 120_000,
            },
            660.0,
            140.0,
        ),
        blk(
            "dly_dn",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 180_000,
            },
            660.0,
            340.0,
        ),
        // Latches
        blk("latch1", BlockType::Latch, 860.0, 140.0),
        blk("latch2", BlockType::Latch, 860.0, 340.0),
        write_pt("w_fan1", "fan_1_cmd", Some(8), 1060.0, 140.0),
        write_pt("w_fan2", "fan_2_cmd", Some(8), 1060.0, 340.0),
    ];
    let wires = vec![
        wire("setpt", "value", "add_up", "a"),
        wire("up_off", "value", "add_up", "b"),
        wire("setpt", "value", "add_dn", "a"),
        wire("dn_off", "value", "add_dn", "b"),
        wire("sp", "value", "cmp_up", "a"),
        wire("add_up", "result", "cmp_up", "b"),
        wire("sp", "value", "cmp_dn", "a"),
        wire("add_dn", "result", "cmp_dn", "b"),
        wire("cmp_up", "result", "dly_up", "value"),
        wire("cmp_dn", "result", "dly_dn", "value"),
        wire("dly_up", "result", "latch1", "set"),
        wire("dly_dn", "result", "latch1", "reset"),
        wire("dly_up", "result", "latch2", "set"),
        wire("dly_dn", "result", "latch2", "reset"),
        wire("latch1", "result", "w_fan1", "value"),
        wire("latch2", "result", "w_fan2", "value"),
    ];
    program(
        "fan_staging",
        "Fan Staging (2-Stage)",
        "Stage supply fans on/off based on duct static pressure with anti-short-cycle delays.",
        5000,
        blocks,
        wires,
    )
}

fn vav_control() -> Program {
    let blocks = vec![
        read("zt", "zone_temp", 60.0, 100.0),
        read("clg_sp", "zone_cooling_sp", 60.0, 240.0),
        read("htg_sp", "zone_heating_sp", 60.0, 380.0),
        constant("min_air", 20.0, 60.0, 520.0),
        // Cooling PID → damper
        blk(
            "pid_clg",
            BlockType::Pid {
                kp: 2.0,
                ki: 0.1,
                kd: 0.0,
                output_min: 0.0,
                output_max: 100.0,
            },
            300.0,
            160.0,
        ),
        // Heating PID → reheat valve
        blk(
            "pid_htg",
            BlockType::Pid {
                kp: 2.0,
                ki: 0.15,
                kd: 0.0,
                output_min: 0.0,
                output_max: 100.0,
            },
            300.0,
            380.0,
        ),
        // Enforce minimum airflow
        blk("max_air", BlockType::Math { op: MathOp::Max }, 540.0, 200.0),
        write_pt("w_damp", "vav_damper_cmd", Some(8), 740.0, 200.0),
        write_pt("w_rht", "reheat_valve_cmd", Some(8), 540.0, 380.0),
    ];
    let wires = vec![
        wire("zt", "value", "pid_clg", "process_variable"),
        wire("clg_sp", "value", "pid_clg", "setpoint"),
        wire("zt", "value", "pid_htg", "process_variable"),
        wire("htg_sp", "value", "pid_htg", "setpoint"),
        wire("pid_clg", "output", "max_air", "a"),
        wire("min_air", "value", "max_air", "b"),
        wire("max_air", "result", "w_damp", "value"),
        wire("pid_htg", "output", "w_rht", "value"),
    ];
    program(
        "vav_control",
        "VAV Box Control",
        "Zone temperature control with variable damper and reheat valve, minimum airflow enforced.",
        5000,
        blocks,
        wires,
    )
}

fn chiller_staging() -> Program {
    let blocks = vec![
        read("chwr", "chw_return_temp", 60.0, 100.0),
        read("chws_sp", "chw_supply_temp_sp", 60.0, 240.0),
        blk("delta", BlockType::Math { op: MathOp::Sub }, 260.0, 160.0),
        constant("up_thr", 8.0, 260.0, 300.0),
        constant("dn_thr", 3.0, 260.0, 400.0),
        blk(
            "cmp_up",
            BlockType::Compare { op: CompareOp::Gt },
            460.0,
            160.0,
        ),
        blk(
            "cmp_dn",
            BlockType::Compare { op: CompareOp::Lt },
            460.0,
            360.0,
        ),
        blk(
            "dly_up",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 300_000,
            },
            660.0,
            160.0,
        ),
        blk(
            "dly_dn",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 300_000,
            },
            660.0,
            360.0,
        ),
        blk("latch1", BlockType::Latch, 860.0, 80.0),
        blk("latch2", BlockType::Latch, 860.0, 220.0),
        blk("latch3", BlockType::Latch, 860.0, 360.0),
        write_pt("w_ch1", "chiller_1_cmd", Some(8), 1060.0, 80.0),
        write_pt("w_ch2", "chiller_2_cmd", Some(8), 1060.0, 220.0),
        write_pt("w_ch3", "chiller_3_cmd", Some(8), 1060.0, 360.0),
    ];
    let wires = vec![
        wire("chwr", "value", "delta", "a"),
        wire("chws_sp", "value", "delta", "b"),
        wire("delta", "result", "cmp_up", "a"),
        wire("up_thr", "value", "cmp_up", "b"),
        wire("delta", "result", "cmp_dn", "a"),
        wire("dn_thr", "value", "cmp_dn", "b"),
        wire("cmp_up", "result", "dly_up", "value"),
        wire("cmp_dn", "result", "dly_dn", "value"),
        wire("dly_up", "result", "latch1", "set"),
        wire("dly_dn", "result", "latch1", "reset"),
        wire("dly_up", "result", "latch2", "set"),
        wire("dly_dn", "result", "latch2", "reset"),
        wire("dly_up", "result", "latch3", "set"),
        wire("dly_dn", "result", "latch3", "reset"),
        wire("latch1", "result", "w_ch1", "value"),
        wire("latch2", "result", "w_ch2", "value"),
        wire("latch3", "result", "w_ch3", "value"),
    ];
    program(
        "chiller_staging",
        "Chiller Staging (3-Stage)",
        "Stage up to three chillers based on load with anti-short-cycle timers.",
        10000,
        blocks,
        wires,
    )
}

fn boiler_oat_reset() -> Program {
    let blocks = vec![
        read("oat", "outdoor_air_temp", 60.0, 160.0),
        // Scale OAT [0..60] → HW SP [180..100] (inverse)
        blk(
            "scale",
            BlockType::Scale {
                in_min: 0.0,
                in_max: 60.0,
                out_min: 180.0,
                out_max: 100.0,
            },
            300.0,
            160.0,
        ),
        // Ramp limit 2°F/min = 0.033°F/s
        blk(
            "ramp",
            BlockType::RampLimit { max_rate: 0.033 },
            500.0,
            160.0,
        ),
        write_pt("w_sp", "hw_supply_temp_sp", Some(8), 700.0, 160.0),
        blk(
            "log",
            BlockType::Log {
                prefix: "HW Reset".into(),
            },
            700.0,
            280.0,
        ),
    ];
    let wires = vec![
        wire("oat", "value", "scale", "value"),
        wire("scale", "result", "ramp", "value"),
        wire("ramp", "result", "w_sp", "value"),
        wire("ramp", "result", "log", "value"),
    ];
    program(
        "boiler_oat_reset",
        "Boiler OAT Reset",
        "Reset hot water supply setpoint from outdoor air temperature with ramp limiting.",
        10000,
        blocks,
        wires,
    )
}

fn changeover() -> Program {
    let blocks = vec![
        read("oat", "outdoor_air_temp", 60.0, 180.0),
        // 4-hour moving average
        blk(
            "avg",
            BlockType::Timing {
                op: TimingOp::MovingAverage,
                period_ms: 14_400_000,
            },
            260.0,
            180.0,
        ),
        constant("clg_en", 65.0, 260.0, 60.0),
        constant("htg_en", 55.0, 260.0, 320.0),
        // avg > 65 → cooling mode
        blk(
            "cmp_clg",
            BlockType::Compare { op: CompareOp::Gt },
            460.0,
            100.0,
        ),
        // avg < 55 → heating mode
        blk(
            "cmp_htg",
            BlockType::Compare { op: CompareOp::Lt },
            460.0,
            280.0,
        ),
        // Latch: set=cooling, reset=heating
        blk("latch_mode", BlockType::Latch, 660.0, 180.0),
        blk(
            "not_clg",
            BlockType::Logic { op: LogicOp::Not },
            860.0,
            280.0,
        ),
        // Delay to prevent rapid changeover
        blk(
            "dly",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 3_600_000,
            },
            860.0,
            100.0,
        ),
        write_pt("w_clg", "system_cooling_mode", None, 1060.0, 100.0),
        write_pt("w_htg", "system_heating_mode", None, 1060.0, 280.0),
    ];
    let wires = vec![
        wire("oat", "value", "avg", "value"),
        wire("avg", "result", "cmp_clg", "a"),
        wire("clg_en", "value", "cmp_clg", "b"),
        wire("avg", "result", "cmp_htg", "a"),
        wire("htg_en", "value", "cmp_htg", "b"),
        wire("cmp_clg", "result", "latch_mode", "set"),
        wire("cmp_htg", "result", "latch_mode", "reset"),
        wire("latch_mode", "result", "dly", "value"),
        wire("dly", "result", "w_clg", "value"),
        wire("latch_mode", "result", "not_clg", "value"),
        wire("not_clg", "result", "w_htg", "value"),
    ];
    program(
        "changeover",
        "Heating/Cooling Changeover",
        "Automatic 2-pipe system mode switch based on smoothed outdoor temperature.",
        30000,
        blocks,
        wires,
    )
}

// ── Energy Management Templates ────────────────────────────────────

fn demand_limiting() -> Program {
    let blocks = vec![
        read("power", "building_power_kw", 60.0, 160.0),
        constant("limit", 500.0, 60.0, 300.0),
        constant("shed_pct", 0.9, 60.0, 420.0),
        constant("rest_pct", 0.8, 60.0, 520.0),
        // shed threshold = limit * 0.9
        blk(
            "mul_shed",
            BlockType::Math { op: MathOp::Mul },
            260.0,
            340.0,
        ),
        // restore threshold = limit * 0.8
        blk(
            "mul_rest",
            BlockType::Math { op: MathOp::Mul },
            260.0,
            480.0,
        ),
        // power > shed?
        blk(
            "cmp_shed",
            BlockType::Compare { op: CompareOp::Gt },
            460.0,
            200.0,
        ),
        // power < restore?
        blk(
            "cmp_rest",
            BlockType::Compare { op: CompareOp::Lt },
            460.0,
            400.0,
        ),
        // Sustained 60s before shedding
        blk(
            "dly",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 60_000,
            },
            660.0,
            200.0,
        ),
        blk("latch_shed", BlockType::Latch, 860.0, 300.0),
        write_pt("w_shed", "demand_shed_active", None, 1060.0, 300.0),
        alarm(
            "alm",
            "building_power_kw",
            "Demand limit approaching",
            860.0,
            140.0,
        ),
    ];
    let wires = vec![
        wire("limit", "value", "mul_shed", "a"),
        wire("shed_pct", "value", "mul_shed", "b"),
        wire("limit", "value", "mul_rest", "a"),
        wire("rest_pct", "value", "mul_rest", "b"),
        wire("power", "value", "cmp_shed", "a"),
        wire("mul_shed", "result", "cmp_shed", "b"),
        wire("power", "value", "cmp_rest", "a"),
        wire("mul_rest", "result", "cmp_rest", "b"),
        wire("cmp_shed", "result", "dly", "value"),
        wire("dly", "result", "latch_shed", "set"),
        wire("cmp_rest", "result", "latch_shed", "reset"),
        wire("latch_shed", "result", "w_shed", "value"),
        wire("cmp_shed", "result", "alm", "condition"),
    ];
    program(
        "demand_limiting",
        "Demand Limiting",
        "Shed loads when building power approaches utility demand limit.",
        5000,
        blocks,
        wires,
    )
}

fn optimal_start() -> Program {
    let blocks = vec![
        read("zt", "zone_temp", 60.0, 60.0),
        read("sp", "zone_occupied_sp", 60.0, 180.0),
        read("countdown", "occupancy_countdown_min", 60.0, 320.0),
        constant("rate", 1.0, 60.0, 440.0),
        // delta = sp - zone_temp
        blk("sub", BlockType::Math { op: MathOp::Sub }, 300.0, 120.0),
        // abs(delta)
        blk("abs", BlockType::Math { op: MathOp::Abs }, 500.0, 120.0),
        // time_needed = abs / rate
        blk("div", BlockType::Math { op: MathOp::Div }, 700.0, 180.0),
        // time_needed >= countdown → start now
        blk(
            "cmp",
            BlockType::Compare { op: CompareOp::Gte },
            900.0,
            240.0,
        ),
        // Hold on for minimum run
        blk(
            "dly",
            BlockType::Timing {
                op: TimingOp::DelayOff,
                period_ms: 600_000,
            },
            1100.0,
            240.0,
        ),
        write_pt("w_start", "ahu_start_cmd", None, 1300.0, 240.0),
        blk(
            "log",
            BlockType::Log {
                prefix: "Optimal Start".into(),
            },
            1100.0,
            360.0,
        ),
    ];
    let wires = vec![
        wire("sp", "value", "sub", "a"),
        wire("zt", "value", "sub", "b"),
        wire("sub", "result", "abs", "value"),
        wire("abs", "result", "div", "a"),
        wire("rate", "value", "div", "b"),
        wire("div", "result", "cmp", "a"),
        wire("countdown", "value", "cmp", "b"),
        wire("cmp", "result", "dly", "value"),
        wire("dly", "result", "w_start", "value"),
        wire("cmp", "result", "log", "value"),
    ];
    program(
        "optimal_start",
        "Optimal Start",
        "Start HVAC early enough to reach setpoint exactly at occupancy time.",
        60000,
        blocks,
        wires,
    )
}

fn night_setback() -> Program {
    let blocks = vec![
        read("occ", "occupancy_status", 60.0, 60.0),
        read("override", "tenant_override", 60.0, 200.0),
        constant("occ_clg", 74.0, 60.0, 340.0),
        constant("occ_htg", 70.0, 60.0, 440.0),
        constant("unocc_clg", 85.0, 60.0, 540.0),
        constant("unocc_htg", 55.0, 60.0, 640.0),
        // Override → one-shot rising edge → delay off (2 hr override window)
        blk("oneshot", BlockType::OneShot, 300.0, 200.0),
        blk(
            "dly_ovr",
            BlockType::Timing {
                op: TimingOp::DelayOff,
                period_ms: 7_200_000,
            },
            500.0,
            200.0,
        ),
        // Occupied OR override active
        blk("or_occ", BlockType::Logic { op: LogicOp::Or }, 700.0, 120.0),
        // Select cooling SP
        blk("sel_clg", BlockType::Select, 900.0, 100.0),
        // Select heating SP
        blk("sel_htg", BlockType::Select, 900.0, 300.0),
        write_pt("w_clg_sp", "effective_cooling_sp", Some(8), 1100.0, 100.0),
        write_pt("w_htg_sp", "effective_heating_sp", Some(8), 1100.0, 300.0),
    ];
    let wires = vec![
        wire("override", "value", "oneshot", "trigger"),
        wire("oneshot", "result", "dly_ovr", "value"),
        wire("occ", "value", "or_occ", "a"),
        wire("dly_ovr", "result", "or_occ", "b"),
        wire("or_occ", "result", "sel_clg", "condition"),
        wire("occ_clg", "value", "sel_clg", "if_true"),
        wire("unocc_clg", "value", "sel_clg", "if_false"),
        wire("or_occ", "result", "sel_htg", "condition"),
        wire("occ_htg", "value", "sel_htg", "if_true"),
        wire("unocc_htg", "value", "sel_htg", "if_false"),
        wire("sel_clg", "result", "w_clg_sp", "value"),
        wire("sel_htg", "result", "w_htg_sp", "value"),
    ];
    program(
        "night_setback",
        "Night Setback / Unoccupied",
        "Widen temperature deadband during unoccupied hours with tenant override.",
        10000,
        blocks,
        wires,
    )
}

// ── Safety & Alarm Templates ───────────────────────────────────────

fn freeze_protect() -> Program {
    let blocks = vec![
        read("mat", "mixed_air_temp", 60.0, 60.0),
        read("dat", "discharge_air_temp", 60.0, 200.0),
        constant("freeze_thr", 38.0, 60.0, 340.0),
        constant("clear_thr", 45.0, 60.0, 460.0),
        // MAT < freeze?
        blk(
            "cmp_mat",
            BlockType::Compare { op: CompareOp::Lt },
            300.0,
            100.0,
        ),
        // DAT < freeze?
        blk(
            "cmp_dat",
            BlockType::Compare { op: CompareOp::Lt },
            300.0,
            240.0,
        ),
        // Either → freeze condition
        blk("or_frz", BlockType::Logic { op: LogicOp::Or }, 500.0, 160.0),
        // MAT > clear → can reset
        blk(
            "cmp_clr",
            BlockType::Compare { op: CompareOp::Gt },
            300.0,
            400.0,
        ),
        // Latch freeze state
        blk("latch_frz", BlockType::Latch, 700.0, 260.0),
        // Outputs
        constant("full_open", 100.0, 700.0, 80.0),
        constant("full_close", 0.0, 700.0, 440.0),
        // Select: freeze → override, else → pass through
        blk("sel_htg", BlockType::Select, 900.0, 120.0),
        blk("sel_damp", BlockType::Select, 900.0, 320.0),
        write_pt("w_htg", "heating_valve_cmd", Some(4), 1100.0, 120.0),
        write_pt("w_damp", "outdoor_air_damper_cmd", Some(4), 1100.0, 320.0),
        alarm(
            "alm_frz",
            "mixed_air_temp",
            "FREEZE PROTECTION ACTIVE",
            900.0,
            500.0,
        ),
    ];
    let wires = vec![
        wire("mat", "value", "cmp_mat", "a"),
        wire("freeze_thr", "value", "cmp_mat", "b"),
        wire("dat", "value", "cmp_dat", "a"),
        wire("freeze_thr", "value", "cmp_dat", "b"),
        wire("cmp_mat", "result", "or_frz", "a"),
        wire("cmp_dat", "result", "or_frz", "b"),
        wire("mat", "value", "cmp_clr", "a"),
        wire("clear_thr", "value", "cmp_clr", "b"),
        wire("or_frz", "result", "latch_frz", "set"),
        wire("cmp_clr", "result", "latch_frz", "reset"),
        // When freeze active → heating 100%, damper 0%
        wire("latch_frz", "result", "sel_htg", "condition"),
        wire("full_open", "value", "sel_htg", "if_true"),
        wire("full_close", "value", "sel_htg", "if_false"),
        wire("latch_frz", "result", "sel_damp", "condition"),
        wire("full_close", "value", "sel_damp", "if_true"),
        wire("full_open", "value", "sel_damp", "if_false"),
        wire("sel_htg", "result", "w_htg", "value"),
        wire("sel_damp", "result", "w_damp", "value"),
        wire("latch_frz", "result", "alm_frz", "condition"),
    ];
    program(
        "freeze_protect",
        "Freeze Protection",
        "Open heating valve and close OA damper when low air temperature detected.",
        2000,
        blocks,
        wires,
    )
}

fn high_low_limit() -> Program {
    let blocks = vec![
        read("pt", "monitored_point", 60.0, 200.0),
        constant("hi", 90.0, 60.0, 60.0),
        constant("lo", 40.0, 60.0, 340.0),
        constant("db", 2.0, 60.0, 480.0),
        // High check
        blk(
            "cmp_hi",
            BlockType::Compare { op: CompareOp::Gt },
            300.0,
            100.0,
        ),
        // Low check
        blk(
            "cmp_lo",
            BlockType::Compare { op: CompareOp::Lt },
            300.0,
            300.0,
        ),
        // Clear thresholds: hi-db, lo+db
        blk("sub_db", BlockType::Math { op: MathOp::Sub }, 300.0, 460.0),
        blk("add_db", BlockType::Math { op: MathOp::Add }, 300.0, 560.0),
        blk(
            "cmp_hi_clr",
            BlockType::Compare { op: CompareOp::Lt },
            500.0,
            460.0,
        ),
        blk(
            "cmp_lo_clr",
            BlockType::Compare { op: CompareOp::Gt },
            500.0,
            560.0,
        ),
        // Delay before alarming (filter noise)
        blk(
            "dly_hi",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 30_000,
            },
            500.0,
            100.0,
        ),
        blk(
            "dly_lo",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 30_000,
            },
            500.0,
            300.0,
        ),
        // Latches
        blk("latch_hi", BlockType::Latch, 700.0, 160.0),
        blk("latch_lo", BlockType::Latch, 700.0, 400.0),
        alarm(
            "alm_hi",
            "monitored_point",
            "HIGH LIMIT EXCEEDED",
            900.0,
            160.0,
        ),
        alarm(
            "alm_lo",
            "monitored_point",
            "LOW LIMIT EXCEEDED",
            900.0,
            400.0,
        ),
    ];
    let wires = vec![
        wire("pt", "value", "cmp_hi", "a"),
        wire("hi", "value", "cmp_hi", "b"),
        wire("pt", "value", "cmp_lo", "a"),
        wire("lo", "value", "cmp_lo", "b"),
        wire("hi", "value", "sub_db", "a"),
        wire("db", "value", "sub_db", "b"),
        wire("lo", "value", "add_db", "a"),
        wire("db", "value", "add_db", "b"),
        wire("pt", "value", "cmp_hi_clr", "a"),
        wire("sub_db", "result", "cmp_hi_clr", "b"),
        wire("pt", "value", "cmp_lo_clr", "a"),
        wire("add_db", "result", "cmp_lo_clr", "b"),
        wire("cmp_hi", "result", "dly_hi", "value"),
        wire("cmp_lo", "result", "dly_lo", "value"),
        wire("dly_hi", "result", "latch_hi", "set"),
        wire("cmp_hi_clr", "result", "latch_hi", "reset"),
        wire("dly_lo", "result", "latch_lo", "set"),
        wire("cmp_lo_clr", "result", "latch_lo", "reset"),
        wire("latch_hi", "result", "alm_hi", "condition"),
        wire("latch_lo", "result", "alm_lo", "condition"),
    ];
    program(
        "high_low_limit",
        "High/Low Limit Monitor",
        "Monitor any analog point against configurable limits with deadband and delay.",
        5000,
        blocks,
        wires,
    )
}

fn runtime_monitor() -> Program {
    let blocks = vec![
        read("status", "equipment_status", 60.0, 100.0),
        constant("maint_hrs", 2000.0, 60.0, 280.0),
        constant("max_cont", 24.0, 60.0, 420.0),
        // CustomScript tracks accumulated and continuous hours
        blk(
            "counter",
            BlockType::CustomScript {
                code: r#"// Track runtime hours
let total = state_get("total_hours");
if total == () { total = 0.0; }
let continuous = state_get("continuous_hours");
if continuous == () { continuous = 0.0; }
let interval_hrs = 1.0 / 3600.0; // 1-second tick
if in1 == true || in1 == 1.0 || in1 == 1 {
    total += interval_hrs;
    continuous += interval_hrs;
} else {
    continuous = 0.0;
}
state_set("total_hours", total);
state_set("continuous_hours", continuous);
out = total;"#
                    .into(),
            },
            300.0,
            100.0,
        ),
        // Read continuous hours via a second script block
        blk(
            "cont_read",
            BlockType::CustomScript {
                code: r#"let c = state_get("continuous_hours");
if c == () { c = 0.0; }
out = c;"#
                    .into(),
            },
            300.0,
            300.0,
        ),
        blk(
            "cmp_maint",
            BlockType::Compare { op: CompareOp::Gt },
            540.0,
            160.0,
        ),
        blk(
            "cmp_cont",
            BlockType::Compare { op: CompareOp::Gt },
            540.0,
            360.0,
        ),
        alarm(
            "alm_maint",
            "equipment_status",
            "Maintenance interval exceeded",
            740.0,
            160.0,
        ),
        alarm(
            "alm_cont",
            "equipment_status",
            "Excessive continuous runtime",
            740.0,
            360.0,
        ),
        blk(
            "w_hrs",
            BlockType::VirtualPoint {
                node_id: "runtime_hours".into(),
            },
            540.0,
            500.0,
        ),
    ];
    let wires = vec![
        wire("status", "value", "counter", "in1"),
        wire("counter", "out", "cmp_maint", "a"),
        wire("maint_hrs", "value", "cmp_maint", "b"),
        wire("cont_read", "out", "cmp_cont", "a"),
        wire("max_cont", "value", "cmp_cont", "b"),
        wire("cmp_maint", "result", "alm_maint", "condition"),
        wire("cmp_cont", "result", "alm_cont", "condition"),
        wire("counter", "out", "w_hrs", "value"),
    ];
    program(
        "runtime_monitor",
        "Equipment Runtime Monitor",
        "Track run hours and alarm on maintenance interval or excessive continuous runtime.",
        1000,
        blocks,
        wires,
    )
}

fn filter_dp() -> Program {
    let blocks = vec![
        read("dp", "filter_dp", 60.0, 100.0),
        read("fan", "fan_status", 60.0, 280.0),
        constant("warn_thr", 1.0, 60.0, 400.0),
        constant("dirty_thr", 1.5, 60.0, 500.0),
        // dp > warning?
        blk(
            "cmp_warn",
            BlockType::Compare { op: CompareOp::Gt },
            300.0,
            100.0,
        ),
        // dp > dirty?
        blk(
            "cmp_dirty",
            BlockType::Compare { op: CompareOp::Gt },
            300.0,
            260.0,
        ),
        // Only alarm when fan running
        blk(
            "and_warn",
            BlockType::Logic { op: LogicOp::And },
            500.0,
            140.0,
        ),
        blk(
            "and_dirty",
            BlockType::Logic { op: LogicOp::And },
            500.0,
            300.0,
        ),
        // Sustain 5 min (filter startup transients)
        blk(
            "dly_warn",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 300_000,
            },
            700.0,
            140.0,
        ),
        blk(
            "dly_dirty",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 300_000,
            },
            700.0,
            300.0,
        ),
        alarm(
            "alm_warn",
            "filter_dp",
            "Filter approaching dirty limit",
            900.0,
            140.0,
        ),
        alarm(
            "alm_dirty",
            "filter_dp",
            "Filter dirty — replace immediately",
            900.0,
            300.0,
        ),
    ];
    let wires = vec![
        wire("dp", "value", "cmp_warn", "a"),
        wire("warn_thr", "value", "cmp_warn", "b"),
        wire("dp", "value", "cmp_dirty", "a"),
        wire("dirty_thr", "value", "cmp_dirty", "b"),
        wire("cmp_warn", "result", "and_warn", "a"),
        wire("fan", "value", "and_warn", "b"),
        wire("cmp_dirty", "result", "and_dirty", "a"),
        wire("fan", "value", "and_dirty", "b"),
        wire("and_warn", "result", "dly_warn", "value"),
        wire("and_dirty", "result", "dly_dirty", "value"),
        wire("dly_warn", "result", "alm_warn", "condition"),
        wire("dly_dirty", "result", "alm_dirty", "condition"),
    ];
    program(
        "filter_dp",
        "Filter DP Monitor",
        "Monitor filter differential pressure and alarm when filter is dirty.",
        5000,
        blocks,
        wires,
    )
}

fn proof_of_operation() -> Program {
    let blocks = vec![
        read("cmd", "equipment_cmd", 60.0, 100.0),
        read("status", "equipment_status", 60.0, 260.0),
        // cmd != status → mismatch
        blk(
            "cmp_neq",
            BlockType::Compare { op: CompareOp::Neq },
            300.0,
            160.0,
        ),
        // Only alarm when we commanded ON and status disagrees
        blk(
            "and_on",
            BlockType::Logic { op: LogicOp::And },
            500.0,
            120.0,
        ),
        // Allow startup time
        blk(
            "dly",
            BlockType::Timing {
                op: TimingOp::DelayOn,
                period_ms: 60_000,
            },
            700.0,
            120.0,
        ),
        alarm(
            "alm",
            "equipment_status",
            "No proof of operation — equipment failed to start",
            900.0,
            120.0,
        ),
        blk(
            "w_fault",
            BlockType::VirtualPoint {
                node_id: "equipment_fault".into(),
            },
            900.0,
            260.0,
        ),
    ];
    let wires = vec![
        wire("cmd", "value", "cmp_neq", "a"),
        wire("status", "value", "cmp_neq", "b"),
        wire("cmd", "value", "and_on", "a"),
        wire("cmp_neq", "result", "and_on", "b"),
        wire("and_on", "result", "dly", "value"),
        wire("dly", "result", "alm", "condition"),
        wire("dly", "result", "w_fault", "value"),
    ];
    program(
        "proof_of_operation",
        "Proof of Operation",
        "Verify equipment started after command; alarm if no proof within timeout.",
        2000,
        blocks,
        wires,
    )
}

// ── Lighting Templates ─────────────────────────────────────────────

fn occ_lighting() -> Program {
    let blocks = vec![
        read("occ", "occupancy_sensor", 60.0, 100.0),
        read("ovr", "manual_override", 60.0, 260.0),
        // 15-min vacancy delay
        blk(
            "dly_off",
            BlockType::Timing {
                op: TimingOp::DelayOff,
                period_ms: 900_000,
            },
            300.0,
            100.0,
        ),
        // Occupied (delayed) OR manual override
        blk("or_on", BlockType::Logic { op: LogicOp::Or }, 500.0, 160.0),
        write_pt("w_light", "lighting_cmd", Some(8), 700.0, 160.0),
    ];
    let wires = vec![
        wire("occ", "value", "dly_off", "value"),
        wire("dly_off", "result", "or_on", "a"),
        wire("ovr", "value", "or_on", "b"),
        wire("or_on", "result", "w_light", "value"),
    ];
    program(
        "occ_lighting",
        "Occupancy Lighting",
        "Control lights based on occupancy sensor with vacancy delay and manual override.",
        1000,
        blocks,
        wires,
    )
}

fn daylight_harvest() -> Program {
    let blocks = vec![
        read("lux", "photosensor_lux", 60.0, 100.0),
        read("occ", "occupancy_status", 60.0, 340.0),
        constant("target", 500.0, 60.0, 220.0),
        constant("zero", 0.0, 60.0, 460.0),
        // deficit = target - measured
        blk("sub", BlockType::Math { op: MathOp::Sub }, 300.0, 140.0),
        // clamp to 0..target
        blk("clamp", BlockType::Math { op: MathOp::Clamp }, 500.0, 140.0),
        constant("c0", 0.0, 300.0, 240.0),
        // Scale deficit [0..500] → [0..100%]
        blk(
            "scale",
            BlockType::Scale {
                in_min: 0.0,
                in_max: 500.0,
                out_min: 0.0,
                out_max: 100.0,
            },
            700.0,
            140.0,
        ),
        // Smooth ramp (no flicker)
        blk("ramp", BlockType::RampLimit { max_rate: 5.0 }, 900.0, 140.0),
        // If unoccupied → 0%
        blk("sel", BlockType::Select, 1100.0, 240.0),
        write_pt("w_dim", "light_dimmer_cmd", Some(8), 1300.0, 240.0),
    ];
    let wires = vec![
        wire("target", "value", "sub", "a"),
        wire("lux", "value", "sub", "b"),
        wire("sub", "result", "clamp", "value"),
        wire("c0", "value", "clamp", "min"),
        wire("target", "value", "clamp", "max"),
        wire("clamp", "result", "scale", "value"),
        wire("scale", "result", "ramp", "value"),
        wire("occ", "value", "sel", "condition"),
        wire("ramp", "result", "sel", "if_true"),
        wire("zero", "value", "sel", "if_false"),
        wire("sel", "result", "w_dim", "value"),
    ];
    program(
        "daylight_harvest",
        "Daylight Harvesting",
        "Dim artificial lights based on photosensor to maintain target illuminance.",
        2000,
        blocks,
        wires,
    )
}

// ── Utility Templates ──────────────────────────────────────────────

fn lead_lag() -> Program {
    let blocks = vec![
        read("enable", "system_enable", 60.0, 60.0),
        read("fault1", "pump_1_fault", 60.0, 200.0),
        read("fault2", "pump_2_fault", 60.0, 340.0),
        // Invert faults → healthy
        blk("not1", BlockType::Logic { op: LogicOp::Not }, 260.0, 200.0),
        blk("not2", BlockType::Logic { op: LogicOp::Not }, 260.0, 340.0),
        // Eligible = enabled AND healthy
        blk("and1", BlockType::Logic { op: LogicOp::And }, 460.0, 120.0),
        blk("and2", BlockType::Logic { op: LogicOp::And }, 460.0, 300.0),
        // CustomScript for lead/lag rotation based on runtime
        blk(
            "rotate",
            BlockType::CustomScript {
                code: r#"// Rotate lead/lag based on accumulated runtime
let lead = state_get("lead");
if lead == () { lead = 1; }
let hrs1 = state_get("hrs1");
if hrs1 == () { hrs1 = 0.0; }
let hrs2 = state_get("hrs2");
if hrs2 == () { hrs2 = 0.0; }
let tick_hrs = 1.0 / 3600.0;
// Count runtime
if in1 == true || in1 == 1 { hrs1 += tick_hrs; }
if in2 == true || in2 == 1 { hrs2 += tick_hrs; }
// Swap at 168h (weekly) difference
if hrs1 - hrs2 > 168.0 { lead = 2; }
if hrs2 - hrs1 > 168.0 { lead = 1; }
// Auto-swap on fault
if in1 != true && in1 != 1 { lead = 2; }
if in2 != true && in2 != 1 { lead = 1; }
state_set("lead", lead);
state_set("hrs1", hrs1);
state_set("hrs2", hrs2);
out = lead;"#
                    .into(),
            },
            660.0,
            200.0,
        ),
        // lead == 1 → pump1 is lead
        constant("c1", 1.0, 660.0, 360.0),
        blk(
            "cmp_lead",
            BlockType::Compare { op: CompareOp::Eq },
            860.0,
            260.0,
        ),
        // Select: if lead=1 → pump1 gets eligible1, pump2 off
        blk("sel1", BlockType::Select, 1060.0, 140.0),
        blk("sel2", BlockType::Select, 1060.0, 340.0),
        write_pt("w_p1", "pump_1_cmd", Some(8), 1260.0, 140.0),
        write_pt("w_p2", "pump_2_cmd", Some(8), 1260.0, 340.0),
        alarm(
            "alm",
            "system_enable",
            "Lead pump faulted — switched to lag",
            860.0,
            420.0,
        ),
    ];
    let wires = vec![
        wire("fault1", "value", "not1", "value"),
        wire("fault2", "value", "not2", "value"),
        wire("enable", "value", "and1", "a"),
        wire("not1", "result", "and1", "b"),
        wire("enable", "value", "and2", "a"),
        wire("not2", "result", "and2", "b"),
        wire("and1", "result", "rotate", "in1"),
        wire("and2", "result", "rotate", "in2"),
        wire("rotate", "out", "cmp_lead", "a"),
        wire("c1", "value", "cmp_lead", "b"),
        // If lead==1: pump1=eligible1, pump2=eligible2
        // If lead==2: pump1=eligible2, pump2=eligible1
        wire("cmp_lead", "result", "sel1", "condition"),
        wire("and1", "result", "sel1", "if_true"),
        wire("and2", "result", "sel1", "if_false"),
        wire("cmp_lead", "result", "sel2", "condition"),
        wire("and2", "result", "sel2", "if_true"),
        wire("and1", "result", "sel2", "if_false"),
        wire("sel1", "result", "w_p1", "value"),
        wire("sel2", "result", "w_p2", "value"),
        wire("fault1", "value", "alm", "condition"),
    ];
    program(
        "lead_lag",
        "Lead/Lag Rotation",
        "Alternate lead/lag equipment to equalize runtime; auto-switch on fault.",
        1000,
        blocks,
        wires,
    )
}

fn duct_static_reset() -> Program {
    let blocks = vec![
        read("vav1", "vav_1_damper_pos", 60.0, 60.0),
        read("vav2", "vav_2_damper_pos", 60.0, 180.0),
        read("vav3", "vav_3_damper_pos", 60.0, 300.0),
        constant("sp_max", 2.0, 60.0, 440.0),
        constant("sp_min", 0.8, 60.0, 540.0),
        constant("request_thr", 70.0, 340.0, 420.0),
        constant("reduce_thr", 50.0, 340.0, 540.0),
        // Max of all damper positions
        blk("max1", BlockType::Math { op: MathOp::Max }, 260.0, 100.0),
        blk("max2", BlockType::Math { op: MathOp::Max }, 460.0, 160.0),
        // max > 70 → need more static
        blk(
            "cmp_up",
            BlockType::Compare { op: CompareOp::Gt },
            560.0,
            340.0,
        ),
        // max < 50 → can reduce
        blk(
            "cmp_dn",
            BlockType::Compare { op: CompareOp::Lt },
            560.0,
            480.0,
        ),
        // Ramp setpoint changes slowly: 0.1 inWC/min ≈ 0.0017/s
        blk("sel_up", BlockType::Select, 760.0, 380.0),
        blk(
            "clamp_sp",
            BlockType::Math { op: MathOp::Clamp },
            960.0,
            380.0,
        ),
        blk(
            "ramp",
            BlockType::RampLimit { max_rate: 0.0017 },
            1160.0,
            380.0,
        ),
        write_pt("w_sp", "duct_static_sp", Some(8), 1360.0, 380.0),
    ];
    let wires = vec![
        wire("vav1", "value", "max1", "a"),
        wire("vav2", "value", "max1", "b"),
        wire("max1", "result", "max2", "a"),
        wire("vav3", "value", "max2", "b"),
        wire("max2", "result", "cmp_up", "a"),
        wire("request_thr", "value", "cmp_up", "b"),
        wire("max2", "result", "cmp_dn", "a"),
        wire("reduce_thr", "value", "cmp_dn", "b"),
        // If dampers requesting → use sp_max, else sp_min (simplified)
        wire("cmp_up", "result", "sel_up", "condition"),
        wire("sp_max", "value", "sel_up", "if_true"),
        wire("sp_min", "value", "sel_up", "if_false"),
        wire("sel_up", "result", "clamp_sp", "value"),
        wire("sp_min", "value", "clamp_sp", "min"),
        wire("sp_max", "value", "clamp_sp", "max"),
        wire("clamp_sp", "result", "ramp", "value"),
        wire("ramp", "result", "w_sp", "value"),
    ];
    program(
        "duct_static_reset",
        "Duct Static Pressure Reset",
        "Reset duct pressure setpoint down when VAV dampers are not requesting.",
        10000,
        blocks,
        wires,
    )
}

fn cascade_pid() -> Program {
    let blocks = vec![
        // Outer loop: zone temp → discharge air temp setpoint
        read("zt", "zone_temp", 60.0, 100.0),
        read("zt_sp", "zone_temp_sp", 60.0, 240.0),
        blk(
            "pid_outer",
            BlockType::Pid {
                kp: 1.0,
                ki: 0.05,
                kd: 0.0,
                output_min: 50.0,
                output_max: 85.0,
            },
            300.0,
            160.0,
        ),
        // Inner loop: discharge air temp → valve/damper
        read("dat", "discharge_air_temp", 300.0, 340.0),
        blk(
            "pid_inner",
            BlockType::Pid {
                kp: 2.0,
                ki: 0.2,
                kd: 0.0,
                output_min: 0.0,
                output_max: 100.0,
            },
            540.0,
            280.0,
        ),
        // Ramp the output for smooth control
        blk("ramp", BlockType::RampLimit { max_rate: 2.0 }, 740.0, 280.0),
        write_pt("w_valve", "control_valve_cmd", Some(8), 940.0, 280.0),
        // Also write the calculated DAT setpoint for visibility
        blk(
            "w_dat_sp",
            BlockType::VirtualPoint {
                node_id: "dat_sp_calculated".into(),
            },
            540.0,
            100.0,
        ),
    ];
    let wires = vec![
        wire("zt", "value", "pid_outer", "process_variable"),
        wire("zt_sp", "value", "pid_outer", "setpoint"),
        // Outer PID output → inner PID setpoint
        wire("pid_outer", "output", "pid_inner", "setpoint"),
        wire("dat", "value", "pid_inner", "process_variable"),
        wire("pid_inner", "output", "ramp", "value"),
        wire("ramp", "result", "w_valve", "value"),
        wire("pid_outer", "output", "w_dat_sp", "value"),
    ];
    program(
        "cascade_pid",
        "Cascade PID Control",
        "Outer-loop PID sets the setpoint for an inner-loop PID (e.g. zone→discharge).",
        5000,
        blocks,
        wires,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::compiler::compile_program;

    #[test]
    fn all_templates_instantiate() {
        for info in catalog() {
            let prog =
                instantiate(info.id).unwrap_or_else(|| panic!("missing template: {}", info.id));
            assert!(!prog.blocks.is_empty(), "empty blocks: {}", info.id);
            assert!(!prog.wires.is_empty(), "empty wires: {}", info.id);
            assert_eq!(prog.name, info.name, "name mismatch: {}", info.id);
        }
    }

    #[test]
    fn all_templates_compile() {
        for info in catalog() {
            let prog = instantiate(info.id).unwrap();
            match compile_program(&prog) {
                Ok(compiled) => {
                    assert!(
                        !compiled.rhai_source.is_empty(),
                        "empty rhai for: {}",
                        info.id,
                    );
                }
                Err(e) => {
                    panic!("template '{}' failed to compile: {:?}", info.id, e);
                }
            }
        }
    }

    #[test]
    fn template_wire_ports_valid() {
        use crate::logic::model::block_ports;
        use std::collections::HashMap;

        for info in catalog() {
            let prog = instantiate(info.id).unwrap();
            let block_map: HashMap<&str, &Block> =
                prog.blocks.iter().map(|b| (b.id.as_str(), b)).collect();

            for (i, w) in prog.wires.iter().enumerate() {
                let from = block_map.get(w.from_block.as_str()).unwrap_or_else(|| {
                    panic!(
                        "{}: wire {}: unknown from_block '{}'",
                        info.id, i, w.from_block
                    )
                });
                let to = block_map.get(w.to_block.as_str()).unwrap_or_else(|| {
                    panic!("{}: wire {}: unknown to_block '{}'", info.id, i, w.to_block)
                });

                let (_, outputs) = block_ports(&from.block_type);
                assert!(
                    outputs.iter().any(|p| p.name == w.from_port),
                    "{}: wire {}: block '{}' has no output port '{}' (available: {:?})",
                    info.id,
                    i,
                    w.from_block,
                    w.from_port,
                    outputs.iter().map(|p| &p.name).collect::<Vec<_>>(),
                );

                let (inputs, _) = block_ports(&to.block_type);
                assert!(
                    inputs.iter().any(|p| p.name == w.to_port),
                    "{}: wire {}: block '{}' has no input port '{}' (available: {:?})",
                    info.id,
                    i,
                    w.to_block,
                    w.to_port,
                    inputs.iter().map(|p| &p.name).collect::<Vec<_>>(),
                );
            }
        }
    }

    #[test]
    fn catalog_has_all_categories() {
        let cats: Vec<TemplateCategory> = catalog().iter().map(|t| t.category).collect();
        for c in TemplateCategory::all() {
            assert!(cats.contains(c), "missing category: {:?}", c);
        }
    }
}
