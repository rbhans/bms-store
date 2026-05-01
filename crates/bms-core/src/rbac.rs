//! Shared role and permission types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserRole {
    Admin,
    Operator,
    Viewer,
}

impl UserRole {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Admin => "Admin",
            Self::Operator => "Operator",
            Self::Viewer => "Viewer",
        }
    }

    pub fn all() -> &'static [UserRole] {
        &[Self::Admin, Self::Operator, Self::Viewer]
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Individual permission that can be granted or denied per role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    WritePoints,
    AcknowledgeAlarms,
    ManageSchedules,
    ManageDiscovery,
    ManagePrograms,
    ManageVirtualPoints,
    ManageUsers,
    ManageNotifications,
    ManageMqtt,
    ManageReports,
    ManageEnergy,
    ManageWebhooks,
    ManageFdd,
    ManageExport,
    ManageCloud,
    ViewAudit,
}

impl Permission {
    pub fn all() -> &'static [Permission] {
        &[
            Self::WritePoints,
            Self::AcknowledgeAlarms,
            Self::ManageSchedules,
            Self::ManageDiscovery,
            Self::ManagePrograms,
            Self::ManageVirtualPoints,
            Self::ManageUsers,
            Self::ManageNotifications,
            Self::ManageMqtt,
            Self::ManageReports,
            Self::ManageEnergy,
            Self::ManageWebhooks,
            Self::ManageFdd,
            Self::ManageExport,
            Self::ManageCloud,
            Self::ViewAudit,
        ]
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::WritePoints => "write_points",
            Self::AcknowledgeAlarms => "acknowledge_alarms",
            Self::ManageSchedules => "manage_schedules",
            Self::ManageDiscovery => "manage_discovery",
            Self::ManagePrograms => "manage_programs",
            Self::ManageVirtualPoints => "manage_virtual_points",
            Self::ManageUsers => "manage_users",
            Self::ManageNotifications => "manage_notifications",
            Self::ManageMqtt => "manage_mqtt",
            Self::ManageReports => "manage_reports",
            Self::ManageEnergy => "manage_energy",
            Self::ManageWebhooks => "manage_webhooks",
            Self::ManageFdd => "manage_fdd",
            Self::ManageExport => "manage_export",
            Self::ManageCloud => "manage_cloud",
            Self::ViewAudit => "view_audit",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::WritePoints => "Write Points",
            Self::AcknowledgeAlarms => "Acknowledge Alarms",
            Self::ManageSchedules => "Manage Schedules",
            Self::ManageDiscovery => "Manage Discovery",
            Self::ManagePrograms => "Manage Programs",
            Self::ManageVirtualPoints => "Manage Virtual Points",
            Self::ManageUsers => "Manage Users",
            Self::ManageNotifications => "Manage Notifications",
            Self::ManageMqtt => "Manage MQTT",
            Self::ManageReports => "Manage Reports",
            Self::ManageEnergy => "Manage Energy Analytics",
            Self::ManageWebhooks => "Manage Webhooks",
            Self::ManageFdd => "Manage FDD",
            Self::ManageExport => "Manage Data Export",
            Self::ManageCloud => "Manage Cloud Bridges",
            Self::ViewAudit => "View Audit Log",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::WritePoints => "Write values to device points (BACnet, Modbus)",
            Self::AcknowledgeAlarms => "Acknowledge active alarms",
            Self::ManageSchedules => "Create, edit, and delete schedules",
            Self::ManageDiscovery => "Accept or ignore discovered devices",
            Self::ManagePrograms => "Create, edit, and delete logic programs",
            Self::ManageVirtualPoints => "Create and write virtual points",
            Self::ManageUsers => "Create, edit, and delete user accounts",
            Self::ManageNotifications => "Manage alarm routing, recipients, and shelving",
            Self::ManageMqtt => "Configure MQTT broker connections and topics",
            Self::ManageReports => "Create, schedule, and view reports",
            Self::ManageEnergy => "Configure energy meters, utility rates, and analytics",
            Self::ManageWebhooks => "Configure webhook subscriptions and delivery",
            Self::ManageFdd => "Manage fault detection rules, bindings, and acknowledge faults",
            Self::ManageExport => "Configure database export connectors and backfill",
            Self::ManageCloud => "Configure cloud platform bridges (AWS, Azure, GCP)",
            Self::ViewAudit => "View audit log entries",
        }
    }

    pub fn from_key(key: &str) -> Option<Permission> {
        match key {
            "write_points" => Some(Self::WritePoints),
            "acknowledge_alarms" => Some(Self::AcknowledgeAlarms),
            "manage_schedules" => Some(Self::ManageSchedules),
            "manage_discovery" => Some(Self::ManageDiscovery),
            "manage_programs" => Some(Self::ManagePrograms),
            "manage_virtual_points" => Some(Self::ManageVirtualPoints),
            "manage_users" => Some(Self::ManageUsers),
            "manage_notifications" => Some(Self::ManageNotifications),
            "manage_mqtt" => Some(Self::ManageMqtt),
            "manage_reports" => Some(Self::ManageReports),
            "manage_energy" => Some(Self::ManageEnergy),
            "manage_webhooks" => Some(Self::ManageWebhooks),
            "manage_fdd" => Some(Self::ManageFdd),
            "manage_export" => Some(Self::ManageExport),
            "manage_cloud" => Some(Self::ManageCloud),
            "view_audit" => Some(Self::ViewAudit),
            _ => None,
        }
    }
}
