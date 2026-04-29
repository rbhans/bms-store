use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

/// Category for organizing FDD rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FddCategory {
    SensorValidation,
    Ahu,
    Vav,
    ChillerPlant,
    HeatPump,
    Economizer,
    General,
}

impl FddCategory {
    pub fn key(&self) -> &'static str {
        match self {
            Self::SensorValidation => "sensor_validation",
            Self::Ahu => "ahu",
            Self::Vav => "vav",
            Self::ChillerPlant => "chiller_plant",
            Self::HeatPump => "heat_pump",
            Self::Economizer => "economizer",
            Self::General => "general",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "sensor_validation" => Some(Self::SensorValidation),
            "ahu" => Some(Self::Ahu),
            "vav" => Some(Self::Vav),
            "chiller_plant" => Some(Self::ChillerPlant),
            "heat_pump" => Some(Self::HeatPump),
            "economizer" => Some(Self::Economizer),
            "general" => Some(Self::General),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Severity
// ---------------------------------------------------------------------------

/// Alarm severity for FDD faults (mirrors alarm store severity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FddSeverity {
    Info,
    Warning,
    Critical,
}

impl FddSeverity {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// AHU operating state
// ---------------------------------------------------------------------------

/// AHU operating state for state-aware FDD rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatingState {
    Heating,
    Economizer,
    EconPlusMech,
    MechCoolingOnly,
    Off,
}

/// Determine AHU operating state from valve/damper/fan positions.
///
/// All values are 0.0–1.0 fractions. A valve or damper is considered active
/// when it exceeds 1 %. The economizer is considered active when the OA damper
/// exceeds the minimum OA position plus a 5 % margin.
pub fn determine_operating_state(
    htg_valve: f64,
    clg_valve: f64,
    oa_damper: f64,
    fan_cmd: f64,
    min_oa_dpr: f64,
) -> OperatingState {
    if fan_cmd < 0.01 {
        return OperatingState::Off;
    }
    let htg_active = htg_valve > 0.01;
    let clg_active = clg_valve > 0.01;
    let econ_active = oa_damper > min_oa_dpr + 0.05;

    if htg_active && !clg_active {
        OperatingState::Heating
    } else if econ_active && clg_active {
        OperatingState::EconPlusMech
    } else if econ_active && !clg_active && !htg_active {
        OperatingState::Economizer
    } else if clg_active && !econ_active {
        OperatingState::MechCoolingOnly
    } else {
        // Transitioning or mixed — treat as Off.
        OperatingState::Off
    }
}

// ---------------------------------------------------------------------------
// RSS sensor tolerance
// ---------------------------------------------------------------------------

/// Metrologically correct combined uncertainty (root-sum-square).
pub fn rss_tolerance(tolerances: &[f64]) -> f64 {
    tolerances.iter().map(|t| t * t).sum::<f64>().sqrt()
}

// ---------------------------------------------------------------------------
// Point references & predicates
// ---------------------------------------------------------------------------

/// Identifies a point by tags within an equipment context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointRef {
    /// All tags must match (AND logic).
    pub tags: Vec<String>,
    /// Human-readable role name for display.
    pub role: String,
}

/// The value to compare a point reading against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PredicateValue {
    Literal(f64),
    PointValue(PointRef),
}

/// Comparison operators for point predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Neq,
}

impl CompareOp {
    pub fn evaluate(&self, left: f64, right: f64) -> bool {
        match self {
            Self::Gt => left > right,
            Self::Lt => left < right,
            Self::Gte => left >= right,
            Self::Lte => left <= right,
            Self::Eq => (left - right).abs() < f64::EPSILON,
            Self::Neq => (left - right).abs() >= f64::EPSILON,
        }
    }
}

/// A single point comparison predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointPredicate {
    pub point_ref: PointRef,
    pub op: CompareOp,
    pub value: PredicateValue,
    /// Sensor tolerance for RSS calculation.
    #[serde(default)]
    pub tolerance: f64,
}

// ---------------------------------------------------------------------------
// FDD conditions
// ---------------------------------------------------------------------------

/// The condition that triggers a fault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FddCondition {
    AllTrue {
        predicates: Vec<PointPredicate>,
        #[serde(default)]
        delay_secs: u64,
        #[serde(default)]
        applicable_states: Option<Vec<OperatingState>>,
    },
    AnyTrue {
        predicates: Vec<PointPredicate>,
        #[serde(default)]
        delay_secs: u64,
        #[serde(default)]
        applicable_states: Option<Vec<OperatingState>>,
    },
    SensorBounds {
        point_ref: PointRef,
        low: f64,
        high: f64,
    },
    StuckValue {
        point_ref: PointRef,
        duration_secs: u64,
        #[serde(default = "default_stuck_tolerance")]
        tolerance: f64,
    },
    CountInWindow {
        point_ref: PointRef,
        threshold_count: u32,
        window_secs: u64,
    },
    ScheduleDeviation {
        point_ref: PointRef,
    },
    Custom {
        script: String,
    },
}

fn default_stuck_tolerance() -> f64 {
    0.1
}

// ---------------------------------------------------------------------------
// FDD parameters (configurable per binding)
// ---------------------------------------------------------------------------

/// Configurable parameters that can be overridden per-binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FddParams {
    #[serde(default = "default_fan_delta")]
    pub delta_t_supply_fan: f64,
    #[serde(default = "default_min_oa_dpr")]
    pub min_oa_damper_pct: f64,
    #[serde(default = "default_temp_tolerance")]
    pub temp_tolerance: f64,
    #[serde(default = "default_pressure_tolerance")]
    pub pressure_tolerance: f64,
    #[serde(default = "default_humidity_tolerance")]
    pub humidity_tolerance: f64,
}

fn default_fan_delta() -> f64 {
    0.5
}
fn default_min_oa_dpr() -> f64 {
    0.1
}
fn default_temp_tolerance() -> f64 {
    1.0
}
fn default_pressure_tolerance() -> f64 {
    0.1
}
fn default_humidity_tolerance() -> f64 {
    3.0
}

impl Default for FddParams {
    fn default() -> Self {
        Self {
            delta_t_supply_fan: default_fan_delta(),
            min_oa_damper_pct: default_min_oa_dpr(),
            temp_tolerance: default_temp_tolerance(),
            pressure_tolerance: default_pressure_tolerance(),
            humidity_tolerance: default_humidity_tolerance(),
        }
    }
}

// ---------------------------------------------------------------------------
// FDD rule definition
// ---------------------------------------------------------------------------

/// A fault detection and diagnostics rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FddRule {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub category: FddCategory,
    /// Required equipment tags (AND logic).
    pub equip_tags: Vec<String>,
    pub severity: FddSeverity,
    pub condition: FddCondition,
    /// Human-readable guidance on what the fault means and what to inspect.
    pub guidance: String,
    pub builtin: bool,
    pub builtin_id: Option<String>,
    pub enabled: bool,
    #[serde(default = "default_confirmation_count")]
    pub confirmation_count: u16,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn default_confirmation_count() -> u16 {
    3
}

// ---------------------------------------------------------------------------
// Rule binding (rule → equipment)
// ---------------------------------------------------------------------------

/// A rule bound to a specific piece of equipment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FddBinding {
    pub id: i64,
    pub rule_id: i64,
    pub equip_id: String,
    pub enabled: bool,
    /// JSON-encoded [`FddParams`] overrides, if any.
    pub config_overrides: Option<String>,
    pub created_ms: i64,
}

// ---------------------------------------------------------------------------
// Faults
// ---------------------------------------------------------------------------

/// State of an active fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FddFaultState {
    Active,
    Acknowledged,
}

impl FddFaultState {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Acknowledged => "acknowledged",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "acknowledged" => Some(Self::Acknowledged),
            _ => None,
        }
    }
}

/// An active fault detected by the FDD engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FddFault {
    pub id: i64,
    pub binding_id: i64,
    pub rule_id: i64,
    pub equip_id: String,
    pub rule_name: String,
    pub severity: FddSeverity,
    pub state: FddFaultState,
    pub detected_ms: i64,
    pub ack_ms: Option<i64>,
    /// JSON-encoded snapshot of point values at detection time.
    pub point_snapshot: String,
    pub guidance: String,
}

/// A historical fault lifecycle event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FddFaultEvent {
    pub id: i64,
    pub fault_id: i64,
    pub binding_id: i64,
    pub rule_id: i64,
    pub equip_id: String,
    pub severity: String,
    pub from_state: String,
    pub to_state: String,
    pub timestamp_ms: i64,
    pub note: Option<String>,
}

/// Query parameters for fault history lookups.
#[derive(Debug, Clone, Default)]
pub struct FddHistoryQuery {
    pub equip_id: Option<String>,
    pub rule_id: Option<i64>,
    pub severity: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub limit: Option<u32>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operating_state_heating() {
        let state = determine_operating_state(0.5, 0.0, 0.05, 1.0, 0.1);
        assert_eq!(state, OperatingState::Heating);
    }

    #[test]
    fn test_operating_state_economizer() {
        // OA damper well above min_oa (0.1) + margin (0.05), no heating, no cooling.
        let state = determine_operating_state(0.0, 0.0, 0.6, 1.0, 0.1);
        assert_eq!(state, OperatingState::Economizer);
    }

    #[test]
    fn test_operating_state_off() {
        // Fan not running.
        let state = determine_operating_state(0.5, 0.5, 0.5, 0.0, 0.1);
        assert_eq!(state, OperatingState::Off);
    }

    #[test]
    fn test_rss_tolerance() {
        let rss = rss_tolerance(&[1.0, 1.0]);
        assert!((rss - std::f64::consts::SQRT_2).abs() < 1e-10);
    }

    #[test]
    fn test_compare_op_evaluate() {
        assert!(CompareOp::Gt.evaluate(2.0, 1.0));
        assert!(!CompareOp::Gt.evaluate(1.0, 2.0));
        assert!(CompareOp::Lt.evaluate(1.0, 2.0));
        assert!(!CompareOp::Lt.evaluate(2.0, 1.0));
        assert!(CompareOp::Gte.evaluate(2.0, 2.0));
        assert!(CompareOp::Gte.evaluate(3.0, 2.0));
        assert!(CompareOp::Lte.evaluate(2.0, 2.0));
        assert!(CompareOp::Lte.evaluate(1.0, 2.0));
        assert!(CompareOp::Eq.evaluate(3.16, 3.16));
        assert!(!CompareOp::Eq.evaluate(3.16, 3.17));
        assert!(CompareOp::Neq.evaluate(1.0, 2.0));
        assert!(!CompareOp::Neq.evaluate(1.0, 1.0));
    }

    #[test]
    fn test_condition_json_roundtrip_all_true() {
        let cond = FddCondition::AllTrue {
            predicates: vec![PointPredicate {
                point_ref: PointRef {
                    tags: vec![
                        "supply".into(),
                        "air".into(),
                        "temp".into(),
                        "sensor".into(),
                    ],
                    role: "SAT".into(),
                },
                op: CompareOp::Lt,
                value: PredicateValue::Literal(55.0),
                tolerance: 1.0,
            }],
            delay_secs: 60,
            applicable_states: Some(vec![OperatingState::Heating]),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let back: FddCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(cond, back);
    }

    #[test]
    fn test_condition_json_roundtrip_sensor_bounds() {
        let cond = FddCondition::SensorBounds {
            point_ref: PointRef {
                tags: vec!["temp".into(), "sensor".into()],
                role: "Zone Temp".into(),
            },
            low: -40.0,
            high: 150.0,
        };
        let json = serde_json::to_string(&cond).unwrap();
        let back: FddCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(cond, back);
    }

    #[test]
    fn test_condition_json_roundtrip_stuck_value() {
        let cond = FddCondition::StuckValue {
            point_ref: PointRef {
                tags: vec!["temp".into(), "sensor".into()],
                role: "OAT".into(),
            },
            duration_secs: 14400,
            tolerance: 0.1,
        };
        let json = serde_json::to_string(&cond).unwrap();
        let back: FddCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(cond, back);
    }

    #[test]
    fn test_condition_json_roundtrip_custom() {
        let cond = FddCondition::Custom {
            script: "read('sat') > read('mat') + 5.0".into(),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let back: FddCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(cond, back);
    }

    #[test]
    fn test_fdd_params_defaults() {
        let params = FddParams::default();
        assert!((params.delta_t_supply_fan - 0.5).abs() < f64::EPSILON);
        assert!((params.min_oa_damper_pct - 0.1).abs() < f64::EPSILON);
        assert!((params.temp_tolerance - 1.0).abs() < f64::EPSILON);
        assert!((params.pressure_tolerance - 0.1).abs() < f64::EPSILON);
        assert!((params.humidity_tolerance - 3.0).abs() < f64::EPSILON);
    }
}
