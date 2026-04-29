use serde::{Deserialize, Serialize};

use crate::config::loader::LoadedScenario;
use crate::config::profile::{PointAccess, PointKind};
use crate::node::{Node, NodeCapabilities, NodeType, ProtocolBinding};
use crate::store::node_store::NodeStore;

/// Auto-create nodes from a loaded scenario.
/// Creates equip nodes for each device and point nodes for each point in the profile.
pub async fn auto_create_nodes(node_store: &NodeStore, loaded: &LoadedScenario) {
    for dev in &loaded.devices {
        // Create equip node for the device
        let equip_node = Node::new(
            dev.instance_id.clone(),
            NodeType::Equip,
            dev.profile.profile.name.clone(),
        );
        if let Err(e) = node_store.create_node(equip_node).await {
            // AlreadyExists is fine on restart
            if !matches!(e, crate::store::node_store::NodeError::AlreadyExists(_)) {
                tracing::error!(node = %dev.instance_id, "Failed to create equip node: {e}");
            }
            continue;
        }

        // Auto-tag the equipment
        let equip_tags = suggest_equip_tags(&dev.profile.profile.name);
        if !equip_tags.is_empty() {
            let _ = node_store.set_tags(&dev.instance_id, equip_tags).await;
        }

        // Create point nodes for each point in the profile
        for pt in &dev.profile.points {
            let point_id = format!("{}/{}", dev.instance_id, pt.id);

            let capabilities = NodeCapabilities::new(
                true,
                matches!(pt.access, PointAccess::Output | PointAccess::Value),
                !pt.history_exclude,
                pt.suggested_alarms.is_some(),
                matches!(pt.access, PointAccess::Output | PointAccess::Value),
            );

            // Build protocol binding from profile mappings
            let binding = build_binding(pt, dev);

            let mut point_node = Node::new(point_id.clone(), NodeType::Point, pt.name.clone())
                .with_parent(dev.instance_id.clone())
                .with_capabilities(capabilities);

            if let Some(b) = binding {
                point_node = point_node.with_binding(b);
            }

            // Set initial value in hot cache
            if let Some(ref initial) = pt.initial_value {
                node_store.init_hot(&point_id, Some(initial.clone()));
            }

            if let Err(e) = node_store.create_node(point_node).await {
                if !matches!(e, crate::store::node_store::NodeError::AlreadyExists(_)) {
                    tracing::error!(node = %point_id, "Failed to create point node: {e}");
                }
                continue;
            }

            // Auto-tag the point
            let point_tags = suggest_point_tags(&pt.name, pt.units.as_deref(), &pt.kind);
            if !point_tags.is_empty() {
                let _ = node_store.set_tags(&point_id, point_tags).await;
            }
        }
    }
}

/// Build a ProtocolBinding from a point's protocol mappings.
fn build_binding(
    pt: &crate::config::profile::Point,
    dev: &crate::config::loader::LoadedDevice,
) -> Option<ProtocolBinding> {
    if let Some(ref protocols) = pt.protocols {
        if let Some(ref bacnet) = protocols.bacnet {
            let device_instance = dev
                .profile
                .defaults
                .as_ref()
                .and_then(|d| d.protocols.as_ref())
                .and_then(|p| p.bacnet.as_ref())
                .and_then(|b| b.device_id)
                .unwrap_or(0);

            return Some(ProtocolBinding::bacnet(
                device_instance,
                &format!("{:?}", bacnet.object_type)
                    .to_lowercase()
                    .replace('_', "-"),
                bacnet.instance,
            ));
        }

        if let Some(ref modbus) = protocols.modbus {
            let defaults = dev
                .profile
                .defaults
                .as_ref()
                .and_then(|d| d.protocols.as_ref())
                .and_then(|p| p.modbus.as_ref());

            return Some(ProtocolBinding::modbus(
                &defaults.and_then(|d| d.host.clone()).unwrap_or_default(),
                defaults.and_then(|d| d.port).unwrap_or(502),
                defaults.and_then(|d| d.unit_id).unwrap_or(1),
                modbus.address,
                &modbus
                    .data_type
                    .as_ref()
                    .map(|dt| format!("{:?}", dt).to_lowercase())
                    .unwrap_or_else(|| "uint16".into()),
                modbus.scale.unwrap_or(1.0),
            ));
        }

        // Handle extra protocols captured via serde flatten
        if let Some((protocol, config)) = protocols.extra.iter().next() {
            return Some(ProtocolBinding::new(protocol.clone(), config.clone()));
        }
    }
    None
}

/// Simple heuristic equipment tagging based on profile name.
fn suggest_equip_tags(name: &str) -> Vec<(String, Option<String>)> {
    let lower = name.to_lowercase();
    let mut tags = vec![("equip".to_string(), None)];

    if lower.contains("ahu") || lower.contains("air handling") {
        tags.push(("ahu".to_string(), None));
        tags.push(("hvac".to_string(), None));
    } else if lower.contains("vav") {
        tags.push(("vav".to_string(), None));
        tags.push(("hvac".to_string(), None));
    } else if lower.contains("rtu") || lower.contains("rooftop") {
        tags.push(("rtu".to_string(), None));
        tags.push(("hvac".to_string(), None));
    } else if lower.contains("chiller") {
        tags.push(("chiller".to_string(), None));
        tags.push(("hvac".to_string(), None));
    } else if lower.contains("boiler") {
        tags.push(("boiler".to_string(), None));
        tags.push(("hvac".to_string(), None));
    } else if lower.contains("pump") {
        tags.push(("pump".to_string(), None));
    } else if lower.contains("fan") {
        tags.push(("fan".to_string(), None));
    }

    tags
}

/// Simple heuristic point tagging based on name and units.
fn suggest_point_tags(
    name: &str,
    units: Option<&str>,
    kind: &PointKind,
) -> Vec<(String, Option<String>)> {
    let lower = name.to_lowercase();
    let mut tags = vec![("point".to_string(), None)];

    // Point kind
    match kind {
        PointKind::Analog => tags.push(("analog".to_string(), None)),
        PointKind::Binary => tags.push(("binary".to_string(), None)),
        PointKind::Multistate => tags.push(("multistate".to_string(), None)),
    }

    // Temperature
    if lower.contains("temp") || lower.contains("tmp") {
        tags.push(("temp".to_string(), None));
        if lower.contains("discharge") || lower.contains("dat") {
            tags.push(("discharge".to_string(), None));
        } else if lower.contains("zone") || lower.contains("zat") || lower.contains("room") {
            tags.push(("zone".to_string(), None));
        } else if lower.contains("outside") || lower.contains("oat") || lower.contains("outdoor") {
            tags.push(("outside".to_string(), None));
        } else if lower.contains("return") || lower.contains("rat") {
            tags.push(("return".to_string(), None));
        } else if lower.contains("mixed") || lower.contains("mat") {
            tags.push(("mixed".to_string(), None));
        }
    }

    // Setpoint
    if lower.contains("setpoint") || lower.contains("sp") {
        tags.push(("sp".to_string(), None));
    }

    // Command/Status
    if lower.contains("command") || lower.contains("cmd") {
        tags.push(("cmd".to_string(), None));
    }
    if lower.contains("status") || lower.contains("sts") {
        tags.push(("sensor".to_string(), None));
    }

    // Fan/Damper/Valve
    if lower.contains("fan") {
        tags.push(("fan".to_string(), None));
    }
    if lower.contains("damper") {
        tags.push(("damper".to_string(), None));
        if lower.contains("outdoor") || lower.contains("outside") || lower.contains("oa") {
            tags.push(("outside".to_string(), None));
        }
    }
    if lower.contains("valve") {
        tags.push(("valve".to_string(), None));
    }

    // Heating / Cooling qualifiers (for FDD rule point resolution)
    if lower.contains("heating") || lower.contains("htg") {
        tags.push(("heating".to_string(), None));
    }
    if lower.contains("cooling") || lower.contains("clg") {
        tags.push(("cooling".to_string(), None));
    }

    // Pressure
    if lower.contains("pressure") || lower.contains("psi") {
        tags.push(("pressure".to_string(), None));
    }

    // Flow
    if lower.contains("flow") || lower.contains("cfm") {
        tags.push(("flow".to_string(), None));
    }

    // Humidity
    if lower.contains("humid") || lower.contains("rh") {
        tags.push(("humidity".to_string(), None));
    }

    // Units tag
    if let Some(u) = units {
        tags.push(("unit".to_string(), Some(u.to_string())));
    }

    tags
}

// ----------------------------------------------------------------
// System Templates
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemTemplate {
    pub name: String,
    pub description: String,
    pub site_hierarchy: Vec<TemplateNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateNode {
    pub id_suffix: String,
    pub node_type: String,
    pub label: String,
    pub children: Vec<TemplateNode>,
}

/// Pre-built system templates.
pub fn builtin_templates() -> Vec<SystemTemplate> {
    vec![
        SystemTemplate {
            name: "Small Office".into(),
            description: "Single-story office building with one AHU and VAVs.".into(),
            site_hierarchy: vec![TemplateNode {
                id_suffix: "site".into(),
                node_type: "site".into(),
                label: "Office Building".into(),
                children: vec![TemplateNode {
                    id_suffix: "floor-1".into(),
                    node_type: "space".into(),
                    label: "Floor 1".into(),
                    children: vec![],
                }],
            }],
        },
        SystemTemplate {
            name: "School".into(),
            description: "Multi-wing school with classrooms, gym, and cafeteria.".into(),
            site_hierarchy: vec![TemplateNode {
                id_suffix: "site".into(),
                node_type: "site".into(),
                label: "School Campus".into(),
                children: vec![
                    TemplateNode {
                        id_suffix: "wing-a".into(),
                        node_type: "space".into(),
                        label: "Wing A - Classrooms".into(),
                        children: vec![],
                    },
                    TemplateNode {
                        id_suffix: "wing-b".into(),
                        node_type: "space".into(),
                        label: "Wing B - Admin".into(),
                        children: vec![],
                    },
                    TemplateNode {
                        id_suffix: "gym".into(),
                        node_type: "space".into(),
                        label: "Gymnasium".into(),
                        children: vec![],
                    },
                    TemplateNode {
                        id_suffix: "cafeteria".into(),
                        node_type: "space".into(),
                        label: "Cafeteria".into(),
                        children: vec![],
                    },
                ],
            }],
        },
        SystemTemplate {
            name: "Hospital".into(),
            description: "Multi-floor hospital with surgical suites, patient rooms, and labs."
                .into(),
            site_hierarchy: vec![TemplateNode {
                id_suffix: "site".into(),
                node_type: "site".into(),
                label: "Hospital".into(),
                children: vec![
                    TemplateNode {
                        id_suffix: "floor-1".into(),
                        node_type: "space".into(),
                        label: "Floor 1 - ER & Lobby".into(),
                        children: vec![],
                    },
                    TemplateNode {
                        id_suffix: "floor-2".into(),
                        node_type: "space".into(),
                        label: "Floor 2 - Patient Rooms".into(),
                        children: vec![],
                    },
                    TemplateNode {
                        id_suffix: "floor-3".into(),
                        node_type: "space".into(),
                        label: "Floor 3 - Surgical".into(),
                        children: vec![],
                    },
                    TemplateNode {
                        id_suffix: "mechanical".into(),
                        node_type: "space".into(),
                        label: "Mechanical Penthouse".into(),
                        children: vec![],
                    },
                ],
            }],
        },
    ]
}

/// Apply a system template to the node store.
pub async fn apply_template_iterative(node_store: &NodeStore, template: &SystemTemplate) {
    // Stack of (template_node, parent_id)
    let mut stack: Vec<(&TemplateNode, Option<String>)> =
        template.site_hierarchy.iter().map(|n| (n, None)).collect();

    while let Some((tnode, parent_id)) = stack.pop() {
        let node_type = NodeType::from_str(&tnode.node_type).unwrap_or(NodeType::Space);
        let mut node = Node::new(tnode.id_suffix.clone(), node_type, tnode.label.clone());
        if let Some(ref pid) = parent_id {
            node = node.with_parent(pid.clone());
        }

        if let Err(e) = node_store.create_node(node).await {
            if !matches!(e, crate::store::node_store::NodeError::AlreadyExists(_)) {
                tracing::error!(node = %tnode.id_suffix, "Failed to create template node: {e}");
            }
        }

        // Push children with this node as parent
        for child in tnode.children.iter().rev() {
            stack.push((child, Some(tnode.id_suffix.clone())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equip_tag_suggestions() {
        let tags = suggest_equip_tags("AHU Single Duct");
        assert!(tags.iter().any(|(t, _)| t == "ahu"));
        assert!(tags.iter().any(|(t, _)| t == "hvac"));
        assert!(tags.iter().any(|(t, _)| t == "equip"));
    }

    #[test]
    fn point_tag_suggestions() {
        let tags = suggest_point_tags("Discharge Air Temp", Some("degF"), &PointKind::Analog);
        assert!(tags.iter().any(|(t, _)| t == "temp"));
        assert!(tags.iter().any(|(t, _)| t == "discharge"));
        assert!(tags.iter().any(|(t, _)| t == "analog"));
        assert!(tags
            .iter()
            .any(|(t, v)| t == "unit" && v.as_deref() == Some("degF")));
    }

    #[test]
    fn builtin_templates_exist() {
        let templates = builtin_templates();
        assert!(templates.len() >= 3);
        assert!(templates.iter().any(|t| t.name == "Small Office"));
        assert!(templates.iter().any(|t| t.name == "School"));
        assert!(templates.iter().any(|t| t.name == "Hospital"));
    }

    #[tokio::test]
    async fn apply_template_creates_nodes() {
        let path = std::env::temp_dir()
            .join("opencrate_template_tests")
            .join(format!("test_{}.db", std::process::id()));
        std::fs::create_dir_all(path.parent().unwrap()).ok();

        let store = crate::store::node_store::start_node_store_with_path(&path);
        let templates = builtin_templates();
        let template = &templates[0]; // Small Office

        apply_template_iterative(&store, template).await;

        let all = store.list_nodes(None, None).await;
        assert!(all.len() >= 2); // site + floor
        assert!(all.iter().any(|n| n.dis == "Office Building"));
        assert!(all.iter().any(|n| n.dis == "Floor 1"));
    }
}
