use serde::{Deserialize, Serialize};

use crate::config::profile::PointValue;
use crate::node::NodeId;

pub type ProgramId = String;
pub type BlockId = String;

// ── Port types ──

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDataType {
    Float,
    Integer,
    Bool,
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortDef {
    pub name: String,
    pub data_type: PortDataType,
    pub default_value: Option<serde_json::Value>,
}

// ── Block operations ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    Abs,
    Clamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicOp {
    And,
    Or,
    Not,
    Xor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Neq,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingOp {
    DelayOn,
    DelayOff,
    MovingAverage,
    RateOfChange,
}

// ── Block types ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockType {
    /// Read a node's current value.
    PointRead { node_id: NodeId },
    /// Emit a constant value.
    Constant { value: PointValue },
    /// Binary math operation (two inputs → one output).
    Math { op: MathOp },
    /// Binary/unary logic operation.
    Logic { op: LogicOp },
    /// Comparison (two inputs → bool output).
    Compare { op: CompareOp },
    /// Select: if condition then a else b.
    Select,
    /// Timing operation with persistent state.
    Timing { op: TimingOp, period_ms: u64 },
    /// PID controller.
    Pid {
        kp: f64,
        ki: f64,
        kd: f64,
        output_min: f64,
        output_max: f64,
    },
    /// Write a value to a node.
    PointWrite {
        node_id: NodeId,
        priority: Option<u8>,
    },
    /// Set a virtual point value.
    VirtualPoint { node_id: NodeId },
    /// Trigger an alarm.
    AlarmTrigger { node_id: NodeId, message: String },
    /// Log a message.
    Log { prefix: String },
    /// Raw Rhai script block.
    CustomScript { code: String },
    /// SR Latch (set/reset flip-flop).
    Latch,
    /// One-shot: emits true for one cycle on rising edge.
    OneShot,
    /// Linear scale: maps input from [in_min..in_max] to [out_min..out_max].
    Scale {
        in_min: f64,
        in_max: f64,
        out_min: f64,
        out_max: f64,
    },
    /// Rate limiter: clamps output change to max_rate per second.
    RampLimit { max_rate: f64 },
}

// ── Block ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: BlockId,
    pub block_type: BlockType,
    /// Visual position (for GUI, unused by runtime).
    pub x: f64,
    pub y: f64,
    /// If false, the block is skipped during compilation (outputs use defaults).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

// ── Wire ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub from_block: BlockId,
    pub from_port: String,
    pub to_block: BlockId,
    pub to_port: String,
}

// ── Trigger ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Run every N milliseconds.
    Periodic { interval_ms: u64 },
    /// Run when any listed node changes value.
    OnChange { node_ids: Vec<NodeId> },
}

// ── Program ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub id: ProgramId,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub blocks: Vec<Block>,
    pub wires: Vec<Wire>,
    /// If set, the compiler is bypassed and this Rhai code runs directly.
    pub rhai_override: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

// ── Execution result ──

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub program_id: ProgramId,
    pub success: bool,
    pub error: Option<String>,
    pub duration_us: u64,
    pub outputs_written: usize,
}

// ── Port generation for each block type ──

/// Returns (inputs, outputs) port definitions for a block type.
pub fn block_ports(bt: &BlockType) -> (Vec<PortDef>, Vec<PortDef>) {
    use PortDataType::*;
    match bt {
        BlockType::PointRead { .. } => (
            vec![],
            vec![PortDef {
                name: "value".into(),
                data_type: Any,
                default_value: None,
            }],
        ),
        BlockType::Constant { .. } => (
            vec![],
            vec![PortDef {
                name: "value".into(),
                data_type: Any,
                default_value: None,
            }],
        ),
        BlockType::Math { op } => {
            let inputs = match op {
                MathOp::Abs => vec![port("value", Float)],
                MathOp::Clamp => vec![port("value", Float), port("min", Float), port("max", Float)],
                _ => vec![port("a", Float), port("b", Float)],
            };
            (
                inputs,
                vec![PortDef {
                    name: "result".into(),
                    data_type: Float,
                    default_value: None,
                }],
            )
        }
        BlockType::Logic { op } => {
            let inputs = match op {
                LogicOp::Not => vec![port("value", Bool)],
                _ => vec![port("a", Bool), port("b", Bool)],
            };
            (
                inputs,
                vec![PortDef {
                    name: "result".into(),
                    data_type: Bool,
                    default_value: None,
                }],
            )
        }
        BlockType::Compare { .. } => (
            vec![port("a", Float), port("b", Float)],
            vec![PortDef {
                name: "result".into(),
                data_type: Bool,
                default_value: None,
            }],
        ),
        BlockType::Select => (
            vec![
                port("condition", Bool),
                port("if_true", Any),
                port("if_false", Any),
            ],
            vec![PortDef {
                name: "result".into(),
                data_type: Any,
                default_value: None,
            }],
        ),
        BlockType::Timing { .. } => (
            vec![port("value", Float)],
            vec![PortDef {
                name: "result".into(),
                data_type: Float,
                default_value: None,
            }],
        ),
        BlockType::Pid { .. } => (
            vec![port("process_variable", Float), port("setpoint", Float)],
            vec![PortDef {
                name: "output".into(),
                data_type: Float,
                default_value: None,
            }],
        ),
        BlockType::PointWrite { .. } | BlockType::VirtualPoint { .. } => {
            (vec![port("value", Any)], vec![])
        }
        BlockType::AlarmTrigger { .. } => (vec![port("condition", Bool)], vec![]),
        BlockType::Log { .. } => (vec![port("value", Any)], vec![]),
        BlockType::CustomScript { .. } => (
            vec![port("in1", Any), port("in2", Any), port("in3", Any)],
            vec![PortDef {
                name: "out".into(),
                data_type: Any,
                default_value: None,
            }],
        ),
        BlockType::Latch => (
            vec![port("set", Bool), port("reset", Bool)],
            vec![PortDef {
                name: "result".into(),
                data_type: Bool,
                default_value: None,
            }],
        ),
        BlockType::OneShot => (
            vec![port("trigger", Bool)],
            vec![PortDef {
                name: "result".into(),
                data_type: Bool,
                default_value: None,
            }],
        ),
        BlockType::Scale { .. } => (
            vec![port("value", Float)],
            vec![PortDef {
                name: "result".into(),
                data_type: Float,
                default_value: None,
            }],
        ),
        BlockType::RampLimit { .. } => (
            vec![port("value", Float)],
            vec![PortDef {
                name: "result".into(),
                data_type: Float,
                default_value: None,
            }],
        ),
    }
}

fn port(name: &str, data_type: PortDataType) -> PortDef {
    PortDef {
        name: name.into(),
        data_type,
        default_value: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_json_roundtrip() {
        let prog = Program {
            id: "test-1".into(),
            name: "Test Program".into(),
            description: "A test".into(),
            enabled: true,
            trigger: Trigger::Periodic { interval_ms: 5000 },
            blocks: vec![
                Block {
                    id: "r1".into(),
                    block_type: BlockType::PointRead {
                        node_id: "ahu-1/oat".into(),
                    },
                    x: 100.0,
                    y: 50.0,
                    enabled: true,
                },
                Block {
                    id: "c1".into(),
                    block_type: BlockType::Constant {
                        value: PointValue::Float(55.0),
                    },
                    x: 100.0,
                    y: 150.0,
                    enabled: true,
                },
                Block {
                    id: "cmp1".into(),
                    block_type: BlockType::Compare { op: CompareOp::Lt },
                    x: 300.0,
                    y: 100.0,
                    enabled: true,
                },
                Block {
                    id: "w1".into(),
                    block_type: BlockType::PointWrite {
                        node_id: "ahu-1/damper-cmd".into(),
                        priority: Some(8),
                    },
                    x: 500.0,
                    y: 100.0,
                    enabled: true,
                },
            ],
            wires: vec![
                Wire {
                    from_block: "r1".into(),
                    from_port: "value".into(),
                    to_block: "cmp1".into(),
                    to_port: "a".into(),
                },
                Wire {
                    from_block: "c1".into(),
                    from_port: "value".into(),
                    to_block: "cmp1".into(),
                    to_port: "b".into(),
                },
                Wire {
                    from_block: "cmp1".into(),
                    from_port: "result".into(),
                    to_block: "w1".into(),
                    to_port: "value".into(),
                },
            ],
            rhai_override: None,
            created_ms: 1000,
            updated_ms: 1000,
        };

        let json = serde_json::to_string_pretty(&prog).unwrap();
        let deser: Program = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.id, "test-1");
        assert_eq!(deser.blocks.len(), 4);
        assert_eq!(deser.wires.len(), 3);
    }

    #[test]
    fn block_ports_math_add() {
        let bt = BlockType::Math { op: MathOp::Add };
        let (inputs, outputs) = block_ports(&bt);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].name, "a");
        assert_eq!(inputs[1].name, "b");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "result");
    }

    #[test]
    fn block_ports_logic_not() {
        let bt = BlockType::Logic { op: LogicOp::Not };
        let (inputs, _) = block_ports(&bt);
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name, "value");
    }

    #[test]
    fn block_ports_pid() {
        let bt = BlockType::Pid {
            kp: 1.0,
            ki: 0.1,
            kd: 0.01,
            output_min: 0.0,
            output_max: 100.0,
        };
        let (inputs, outputs) = block_ports(&bt);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].name, "process_variable");
        assert_eq!(inputs[1].name, "setpoint");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "output");
    }

    #[test]
    fn trigger_json_variants() {
        let periodic = Trigger::Periodic { interval_ms: 5000 };
        let json = serde_json::to_string(&periodic).unwrap();
        assert!(json.contains("periodic"));

        let on_change = Trigger::OnChange {
            node_ids: vec!["ahu-1/oat".into()],
        };
        let json = serde_json::to_string(&on_change).unwrap();
        assert!(json.contains("on_change"));
    }
}
