use crate::store::report_store::{
    AggregationMode, PointSelector, ReportConfig, ReportSection, SectionType, TimeRangeKind,
};

/// Pre-built Energy Summary report: consumption + demand for power-tagged points.
pub fn energy_summary_template() -> ReportConfig {
    ReportConfig {
        time_range: TimeRangeKind::Last7Days,
        sections: vec![
            ReportSection {
                title: "Energy Consumption".to_string(),
                section_type: SectionType::EnergyConsumption,
                point_selector: PointSelector::ByTag("power".to_string()),
                aggregation: AggregationMode::Daily,
            },
            ReportSection {
                title: "Demand Analysis".to_string(),
                section_type: SectionType::DemandSummary,
                point_selector: PointSelector::ByTag("power".to_string()),
                aggregation: AggregationMode::Raw,
            },
        ],
    }
}

/// Pre-built Alarm Summary report: alarm counts by severity + critical alarm list.
pub fn alarm_summary_template() -> ReportConfig {
    ReportConfig {
        time_range: TimeRangeKind::Last7Days,
        sections: vec![
            ReportSection {
                title: "Alarm Overview".to_string(),
                section_type: SectionType::AlarmSummary,
                point_selector: PointSelector::ByTag("".to_string()), // all alarms
                aggregation: AggregationMode::Raw,
            },
            ReportSection {
                title: "Critical & Life Safety Alarms".to_string(),
                section_type: SectionType::AlarmList,
                point_selector: PointSelector::ByTag("".to_string()), // filtered by renderer
                aggregation: AggregationMode::Raw,
            },
        ],
    }
}

/// Pre-built Comfort Compliance report: temperature history summary.
pub fn comfort_compliance_template() -> ReportConfig {
    ReportConfig {
        time_range: TimeRangeKind::Last7Days,
        sections: vec![ReportSection {
            title: "Zone Temperature Summary".to_string(),
            section_type: SectionType::HistorySummary,
            point_selector: PointSelector::ByTag("temp".to_string()),
            aggregation: AggregationMode::Hourly,
        }],
    }
}

/// Pre-built Equipment Runtime report: on-time percentage for binary command points.
pub fn equipment_runtime_template() -> ReportConfig {
    ReportConfig {
        time_range: TimeRangeKind::Last7Days,
        sections: vec![
            ReportSection {
                title: "Equipment Runtime".to_string(),
                section_type: SectionType::RuntimeSummary,
                point_selector: PointSelector::ByTag("cmd".to_string()),
                aggregation: AggregationMode::Raw,
            },
            ReportSection {
                title: "Current Equipment Status".to_string(),
                section_type: SectionType::CurrentValues,
                point_selector: PointSelector::ByTag("cmd".to_string()),
                aggregation: AggregationMode::Raw,
            },
        ],
    }
}

/// Returns the default template config for a given report type.
pub fn template_for_type(report_type: &crate::store::report_store::ReportType) -> ReportConfig {
    use crate::store::report_store::ReportType;
    match report_type {
        ReportType::EnergySummary => energy_summary_template(),
        ReportType::AlarmSummary => alarm_summary_template(),
        ReportType::ComfortCompliance => comfort_compliance_template(),
        ReportType::EquipmentRuntime => equipment_runtime_template(),
        ReportType::Custom => ReportConfig {
            time_range: TimeRangeKind::Last24Hours,
            sections: vec![],
        },
    }
}
