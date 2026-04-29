use super::model::{
    CompareOp, FddCategory, FddCondition, FddRule, FddSeverity, OperatingState, PointPredicate,
    PointRef, PredicateValue,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pt(tags: &[&str], role: &str) -> PointRef {
    PointRef {
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        role: role.to_string(),
    }
}

fn pred_literal(tags: &[&str], role: &str, op: CompareOp, val: f64, tol: f64) -> PointPredicate {
    PointPredicate {
        point_ref: pt(tags, role),
        op,
        value: PredicateValue::Literal(val),
        tolerance: tol,
    }
}

fn pred_point(
    tags: &[&str],
    role: &str,
    op: CompareOp,
    rhs_tags: &[&str],
    rhs_role: &str,
    tol: f64,
) -> PointPredicate {
    PointPredicate {
        point_ref: pt(tags, role),
        op,
        value: PredicateValue::PointValue(pt(rhs_tags, rhs_role)),
        tolerance: tol,
    }
}

fn rule(
    builtin_id: &str,
    name: &str,
    description: &str,
    category: FddCategory,
    equip_tags: &[&str],
    severity: FddSeverity,
    condition: FddCondition,
    guidance: &str,
    confirmation_count: u16,
) -> FddRule {
    FddRule {
        id: 0,
        name: name.to_string(),
        description: description.to_string(),
        category,
        equip_tags: equip_tags.iter().map(|t| (*t).to_string()).collect(),
        severity,
        condition,
        guidance: guidance.to_string(),
        builtin: true,
        builtin_id: Some(builtin_id.to_string()),
        enabled: true,
        confirmation_count,
        created_ms: 0,
        updated_ms: 0,
    }
}

// ---------------------------------------------------------------------------
// Built-in rules
// ---------------------------------------------------------------------------

/// Returns the 19 built-in FDD rules covering ASHRAE GL36 AHU faults,
/// sensor validation, equipment protection, and economizer diagnostics.
pub fn builtin_fdd_rules() -> Vec<FddRule> {
    vec![
        // -----------------------------------------------------------------
        // AHU rules (GL36)
        // -----------------------------------------------------------------

        // FC1 — Duct static pressure too low at maximum fan speed.
        rule(
            "ahu_fc1",
            "AHU FC1: Duct static pressure low at max fan",
            "Supply fan is at maximum speed but duct static pressure remains below setpoint.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["fan", "speed", "sensor"],
                        "Fan Speed",
                        CompareOp::Gte,
                        0.95,
                        0.0,
                    ),
                    pred_point(
                        &["duct", "static", "pressure", "sensor"],
                        "Duct Pressure",
                        CompareOp::Lt,
                        &["duct", "static", "pressure", "sp"],
                        "Duct Pressure SP",
                        0.1,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![
                    OperatingState::Heating,
                    OperatingState::Economizer,
                    OperatingState::EconPlusMech,
                    OperatingState::MechCoolingOnly,
                ]),
            },
            "The supply fan cannot maintain duct static pressure setpoint. \
             Inspect the fan belt, VFD, ductwork for obstructions, and verify the setpoint is reasonable.",
            3,
        ),

        // FC2 — MAT too low (below both RAT and OAT).
        rule(
            "ahu_fc2",
            "AHU FC2: Mixed air temperature too low",
            "Mixed air temperature is below both the return and outdoor air temperatures, \
             indicating a sensor error or unexpected air leakage.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["fan", "run", "cmd"],
                        "Fan Command",
                        CompareOp::Gte,
                        0.01,
                        0.0,
                    ),
                    pred_point(
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        CompareOp::Lt,
                        &["return", "air", "temp", "sensor"],
                        "RAT",
                        1.0,
                    ),
                    pred_point(
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        CompareOp::Lt,
                        &["outside", "air", "temp", "sensor"],
                        "OAT",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: None,
            },
            "MAT is below both RAT and OAT which is physically impossible under normal conditions. \
             Check the MAT sensor calibration, wiring, and look for cold air infiltration near the sensor.",
            3,
        ),

        // FC3 — MAT too high (above both RAT and OAT).
        rule(
            "ahu_fc3",
            "AHU FC3: Mixed air temperature too high",
            "Mixed air temperature is above both the return and outdoor air temperatures, \
             indicating a sensor error or unexpected heat source.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["fan", "run", "cmd"],
                        "Fan Command",
                        CompareOp::Gte,
                        0.01,
                        0.0,
                    ),
                    pred_point(
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        CompareOp::Gt,
                        &["return", "air", "temp", "sensor"],
                        "RAT",
                        1.0,
                    ),
                    pred_point(
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        CompareOp::Gt,
                        &["outside", "air", "temp", "sensor"],
                        "OAT",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: None,
            },
            "MAT is above both RAT and OAT which is physically impossible under normal conditions. \
             Check the MAT sensor for calibration drift, nearby heat sources, or wiring issues.",
            3,
        ),

        // FC4 — Hunting / excessive state changes.
        rule(
            "ahu_fc4",
            "AHU FC4: Excessive control hunting",
            "Fan command is oscillating excessively, indicating unstable control loop tuning.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::CountInWindow {
                point_ref: pt(&["fan", "run", "cmd"], "Fan Command"),
                threshold_count: 10,
                window_secs: 3600,
            },
            "The fan command changed direction more than 10 times in one hour. \
             Review PID tuning parameters, check for sensor noise, and verify setpoints are not conflicting.",
            3,
        ),

        // FC5 — SAT too low when heating.
        rule(
            "ahu_fc5",
            "AHU FC5: Supply air temp too low in heating",
            "Supply air temperature is below mixed air temperature (adjusted for fan heat) \
             while the heating valve is open, suggesting a stuck or failed heating valve.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["heating", "valve", "cmd"],
                        "Heating Valve",
                        CompareOp::Gt,
                        0.01,
                        0.0,
                    ),
                    pred_point(
                        &["supply", "air", "temp", "sensor"],
                        "SAT",
                        CompareOp::Lte,
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![OperatingState::Heating]),
            },
            "SAT is not rising above MAT despite active heating. \
             Inspect the heating coil valve actuator, hot water supply temperature, and air flow across the coil.",
            3,
        ),

        // FC7 — SAT below setpoint at full heating.
        rule(
            "ahu_fc7",
            "AHU FC7: SAT below setpoint at full heating",
            "Supply air temperature remains below setpoint even with the heating valve \
             fully open, indicating insufficient heating capacity.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["heating", "valve", "cmd"],
                        "Heating Valve",
                        CompareOp::Gte,
                        0.90,
                        0.0,
                    ),
                    pred_point(
                        &["supply", "air", "temp", "sensor"],
                        "SAT",
                        CompareOp::Lt,
                        &["supply", "air", "temp", "sp"],
                        "SAT SP",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![OperatingState::Heating]),
            },
            "Heating valve is over 90% open but SAT has not reached setpoint. \
             Check hot water supply temperature, coil fouling, and verify the heating plant is operational.",
            3,
        ),

        // FC8 — SAT/MAT mismatch in economizer mode.
        rule(
            "ahu_fc8",
            "AHU FC8: SAT/MAT mismatch in economizer",
            "Supply air temperature deviates significantly from mixed air temperature \
             (adjusted for fan heat) in economizer mode, suggesting a sensor issue or \
             unexpected coil operation.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    // We use SAT > MAT with RSS tolerance as a proxy for |SAT - fan_delta - MAT| > RSS.
                    // The engine applies fan_delta from FddParams during evaluation.
                    pred_point(
                        &["supply", "air", "temp", "sensor"],
                        "SAT",
                        CompareOp::Gt,
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![OperatingState::Economizer]),
            },
            "In economizer mode, SAT should closely track MAT plus fan motor heat. \
             Verify SAT and MAT sensor calibration, and check that no coil valves are leaking through.",
            3,
        ),

        // FC9 — OAT too warm for free cooling.
        rule(
            "ahu_fc9",
            "AHU FC9: OAT too warm for free cooling",
            "Outdoor air temperature exceeds the supply air setpoint, yet the system is \
             attempting economizer-only cooling without mechanical assistance.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_point(
                        &["outside", "air", "temp", "sensor"],
                        "OAT",
                        CompareOp::Gt,
                        &["supply", "air", "temp", "sp"],
                        "SAT SP",
                        1.0,
                    ),
                    pred_literal(
                        &["outside", "air", "damper", "cmd"],
                        "OA Damper",
                        CompareOp::Gt,
                        0.15,
                        0.0,
                    ),
                    pred_literal(
                        &["cooling", "valve", "cmd"],
                        "Cooling Valve",
                        CompareOp::Lte,
                        0.01,
                        0.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![OperatingState::Economizer]),
            },
            "OAT is above the SAT setpoint so free cooling cannot satisfy the load. \
             The economizer should be disabled and mechanical cooling engaged. Check changeover logic.",
            3,
        ),

        // FC10 — OAT/MAT mismatch in econ+mech mode.
        rule(
            "ahu_fc10",
            "AHU FC10: OAT/MAT mismatch in econ+mech",
            "Mixed air temperature differs significantly from outdoor air temperature \
             when the OA damper is nearly fully open in economizer-plus-mechanical mode.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["outside", "air", "damper", "cmd"],
                        "OA Damper",
                        CompareOp::Gte,
                        0.90,
                        0.0,
                    ),
                    pred_point(
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        CompareOp::Neq,
                        &["outside", "air", "temp", "sensor"],
                        "OAT",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![OperatingState::EconPlusMech]),
            },
            "With the OA damper above 90%, MAT should be close to OAT. \
             Check for damper linkage issues, actuator failure, or return air leakage past the damper.",
            3,
        ),

        // FC11 — OAT/MAT mismatch in economizer-only mode.
        rule(
            "ahu_fc11",
            "AHU FC11: OAT/MAT mismatch in economizer",
            "Mixed air temperature differs significantly from outdoor air temperature \
             when the OA damper is nearly fully open in pure economizer mode.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["outside", "air", "damper", "cmd"],
                        "OA Damper",
                        CompareOp::Gte,
                        0.90,
                        0.0,
                    ),
                    pred_point(
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        CompareOp::Neq,
                        &["outside", "air", "temp", "sensor"],
                        "OAT",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![OperatingState::Economizer]),
            },
            "With the OA damper above 90% in economizer mode, MAT should track OAT closely. \
             Inspect the OA and RA damper actuators and linkages for failure or slippage.",
            3,
        ),

        // FC12 — SAT above MAT in cooling mode.
        rule(
            "ahu_fc12",
            "AHU FC12: SAT above MAT in cooling",
            "Supply air temperature is above mixed air temperature (plus fan heat and RSS \
             tolerance) during cooling, indicating a failed or leaking cooling valve.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_point(
                        &["supply", "air", "temp", "sensor"],
                        "SAT",
                        CompareOp::Gt,
                        &["mixed", "air", "temp", "sensor"],
                        "MAT",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![
                    OperatingState::MechCoolingOnly,
                    OperatingState::EconPlusMech,
                ]),
            },
            "SAT should be below MAT during active cooling. \
             Check the cooling coil valve, chilled water supply temperature, and verify coil is not fouled.",
            3,
        ),

        // FC13 — SAT above setpoint at full cooling.
        rule(
            "ahu_fc13",
            "AHU FC13: SAT above setpoint at full cooling",
            "Supply air temperature remains above setpoint even with the cooling valve \
             fully open, indicating insufficient cooling capacity.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["cooling", "valve", "cmd"],
                        "Cooling Valve",
                        CompareOp::Gte,
                        0.90,
                        0.0,
                    ),
                    pred_point(
                        &["supply", "air", "temp", "sensor"],
                        "SAT",
                        CompareOp::Gt,
                        &["supply", "air", "temp", "sp"],
                        "SAT SP",
                        1.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: Some(vec![
                    OperatingState::MechCoolingOnly,
                    OperatingState::EconPlusMech,
                ]),
            },
            "Cooling valve is over 90% open but SAT has not reached setpoint. \
             Check chilled water supply temperature, coil fouling, and verify the chiller plant is operational.",
            3,
        ),

        // Simultaneous heating and cooling.
        rule(
            "ahu_simultaneous",
            "AHU: Simultaneous heating and cooling",
            "Both the heating and cooling valves are open above 20% at the same time, \
             wasting energy.",
            FddCategory::Ahu,
            &["ahu"],
            FddSeverity::Critical,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["heating", "valve", "cmd"],
                        "Heating Valve",
                        CompareOp::Gt,
                        0.20,
                        0.0,
                    ),
                    pred_literal(
                        &["cooling", "valve", "cmd"],
                        "Cooling Valve",
                        CompareOp::Gt,
                        0.20,
                        0.0,
                    ),
                ],
                delay_secs: 300,
                applicable_states: None,
            },
            "Simultaneous heating and cooling wastes significant energy. \
             Check the control sequence deadband, PID tuning, and verify valve actuators are functioning correctly.",
            3,
        ),

        // -----------------------------------------------------------------
        // Sensor validation
        // -----------------------------------------------------------------

        // Sensor bounds — temperature outside physically plausible range.
        rule(
            "sensor_bounds_temp",
            "Sensor: Temperature out of bounds",
            "Temperature sensor reading is outside the physically plausible range of \
             -40 to 150 degF.",
            FddCategory::SensorValidation,
            &["equip"],
            FddSeverity::Warning,
            FddCondition::SensorBounds {
                point_ref: pt(&["temp", "sensor"], "Temperature"),
                low: -40.0,
                high: 150.0,
            },
            "A temperature reading outside -40 to 150 degF almost certainly indicates a sensor failure. \
             Check wiring, replace the sensor, and verify the analog input scaling.",
            3,
        ),

        // Stuck value — sensor unchanged for 4 hours.
        rule(
            "sensor_stuck",
            "Sensor: Stuck value",
            "Sensor reading has not changed within tolerance for 4 hours, indicating \
             a possible sensor failure or disconnection.",
            FddCategory::SensorValidation,
            &["equip"],
            FddSeverity::Warning,
            FddCondition::StuckValue {
                point_ref: pt(&["sensor"], "Sensor"),
                duration_secs: 14400,
                tolerance: 0.1,
            },
            "A sensor that does not change for 4 hours is likely failed or disconnected. \
             Inspect the sensor wiring, verify the analog input channel, and check the controller.",
            3,
        ),

        // -----------------------------------------------------------------
        // Equipment protection
        // -----------------------------------------------------------------

        // Short cycling — too many starts in one hour.
        rule(
            "equip_short_cycling",
            "Equipment: Short cycling",
            "Equipment has started more than 6 times in one hour, which can cause \
             mechanical wear and increased energy consumption.",
            FddCategory::General,
            &["equip"],
            FddSeverity::Critical,
            FddCondition::CountInWindow {
                point_ref: pt(&["run", "cmd"], "Run Command"),
                threshold_count: 6,
                window_secs: 3600,
            },
            "Short cycling accelerates mechanical wear on compressors and motors. \
             Add or increase the minimum on/off timers, check for undersized equipment, and review control deadbands.",
            3,
        ),

        // -----------------------------------------------------------------
        // Economizer diagnostics
        // -----------------------------------------------------------------

        // Economizer not engaging when OAT is favorable.
        rule(
            "econ_not_economizing",
            "Economizer: Not engaging when favorable",
            "Outdoor air temperature is well below return air temperature but the OA \
             damper remains near minimum, missing free cooling opportunity.",
            FddCategory::Economizer,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_literal(
                        &["fan", "run", "cmd"],
                        "Fan Command",
                        CompareOp::Gte,
                        0.01,
                        0.0,
                    ),
                    pred_point(
                        &["outside", "air", "temp", "sensor"],
                        "OAT",
                        CompareOp::Lt,
                        &["return", "air", "temp", "sensor"],
                        "RAT",
                        5.0,
                    ),
                    pred_literal(
                        &["outside", "air", "damper", "cmd"],
                        "OA Damper",
                        CompareOp::Lt,
                        0.30,
                        0.0,
                    ),
                ],
                delay_secs: 600,
                applicable_states: None,
            },
            "OAT is more than 5 degF below RAT but the economizer damper is not opening. \
             Check the economizer changeover logic, damper actuator, and outdoor air sensor.",
            3,
        ),

        // Mechanical cooling when economizer should suffice.
        rule(
            "econ_mech_when_econ_available",
            "Economizer: Mechanical cooling when free cooling available",
            "Outdoor air temperature is below the supply air setpoint yet mechanical \
             cooling is active and the OA damper is near minimum, wasting energy.",
            FddCategory::Economizer,
            &["ahu"],
            FddSeverity::Warning,
            FddCondition::AllTrue {
                predicates: vec![
                    pred_point(
                        &["outside", "air", "temp", "sensor"],
                        "OAT",
                        CompareOp::Lt,
                        &["supply", "air", "temp", "sp"],
                        "SAT SP",
                        0.0,
                    ),
                    pred_literal(
                        &["cooling", "valve", "cmd"],
                        "Cooling Valve",
                        CompareOp::Gt,
                        0.01,
                        0.0,
                    ),
                    pred_literal(
                        &["outside", "air", "damper", "cmd"],
                        "OA Damper",
                        CompareOp::Lt,
                        0.15,
                        0.0,
                    ),
                ],
                delay_secs: 600,
                applicable_states: None,
            },
            "OAT is below the SAT setpoint so the economizer alone could satisfy the cooling load. \
             Check the economizer enable logic, high-limit lockout setting, and OA damper actuator.",
            3,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_rules_count() {
        let rules = builtin_fdd_rules();
        assert_eq!(rules.len(), 18);
    }

    #[test]
    fn test_all_rules_have_builtin_id() {
        for rule in builtin_fdd_rules() {
            assert!(
                rule.builtin,
                "Rule '{}' should be marked builtin",
                rule.name
            );
            assert!(
                rule.builtin_id.is_some(),
                "Rule '{}' should have a builtin_id",
                rule.name
            );
        }
    }

    #[test]
    fn test_all_rules_have_guidance() {
        for rule in builtin_fdd_rules() {
            assert!(
                !rule.guidance.is_empty(),
                "Rule '{}' should have guidance text",
                rule.name
            );
        }
    }

    #[test]
    fn test_all_rules_have_equip_tags() {
        for rule in builtin_fdd_rules() {
            assert!(
                !rule.equip_tags.is_empty(),
                "Rule '{}' should have at least one equip tag",
                rule.name
            );
        }
    }

    #[test]
    fn test_builtin_ids_unique() {
        let rules = builtin_fdd_rules();
        let ids: Vec<&str> = rules
            .iter()
            .map(|r| r.builtin_id.as_deref().unwrap())
            .collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len(), "Duplicate builtin_id found");
    }

    #[test]
    fn test_rules_json_roundtrip() {
        for rule in builtin_fdd_rules() {
            let json = serde_json::to_string(&rule).unwrap();
            let back: FddRule = serde_json::from_str(&json).unwrap();
            assert_eq!(rule, back, "Round-trip failed for rule '{}'", rule.name);
        }
    }
}
