/// Default topic pattern for point value events.
pub const DEFAULT_VALUE_PATTERN: &str = "opencrate/{device_id}/{point_id}/value";
/// Default topic pattern for alarm events.
/// Note: severity is not carried on EventBus AlarmRaised/Cleared events,
/// so the default pattern uses device_id/point_id instead.
pub const DEFAULT_ALARM_PATTERN: &str = "opencrate/alarms/{device_id}/{point_id}";
/// Default topic pattern for device status events.
pub const DEFAULT_STATUS_PATTERN: &str = "opencrate/status/{device_key}";

/// Context for resolving topic template variables.
#[derive(Default)]
pub struct TopicContext<'a> {
    pub site_id: &'a str,
    pub device_id: &'a str,
    pub point_id: &'a str,
    pub node_id: &'a str,
    pub protocol: &'a str,
    pub severity: &'a str,
    pub device_key: &'a str,
}

/// Sanitize a variable value for use in an MQTT topic.
/// Replaces `/` with `-` to prevent accidental topic nesting.
/// Strips `#`, `+`, and null bytes (MQTT wildcard/reserved chars).
fn sanitize_value(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '#' && *c != '+' && *c != '\0')
        .map(|c| if c == '/' { '-' } else { c })
        .collect()
}

/// Resolve a topic pattern by replacing `{variable}` placeholders with values from ctx.
/// Unknown variables become `_unknown_`.
pub fn resolve_topic(pattern: &str, ctx: &TopicContext) -> String {
    let mut result = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            // Collect variable name until '}'
            let mut var = String::new();
            for vc in chars.by_ref() {
                if vc == '}' {
                    break;
                }
                var.push(vc);
            }
            let value = match var.as_str() {
                "site_id" => ctx.site_id,
                "device_id" => ctx.device_id,
                "point_id" => ctx.point_id,
                "node_id" => ctx.node_id,
                "protocol" => ctx.protocol,
                "severity" => ctx.severity,
                "device_key" => ctx.device_key,
                _ => "_unknown_",
            };
            if value.is_empty() {
                result.push('_');
            } else {
                result.push_str(&sanitize_value(value));
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Build a TopicContext from a ValueChanged event.
/// node_id format is typically `"device_id/point_id"`.
pub fn context_from_value<'a>(node_id: &'a str) -> TopicContext<'a> {
    let (device_id, point_id) = match node_id.find('/') {
        Some(idx) => (&node_id[..idx], &node_id[idx + 1..]),
        None => (node_id, ""),
    };
    TopicContext {
        node_id,
        device_id,
        point_id,
        ..Default::default()
    }
}

/// Build a TopicContext from an alarm event.
pub fn context_from_alarm<'a>(node_id: &'a str, severity: &'a str) -> TopicContext<'a> {
    let (device_id, point_id) = match node_id.find('/') {
        Some(idx) => (&node_id[..idx], &node_id[idx + 1..]),
        None => (node_id, ""),
    };
    TopicContext {
        node_id,
        device_id,
        point_id,
        severity,
        ..Default::default()
    }
}

/// Build a TopicContext from a device status event.
pub fn context_from_status<'a>(device_key: &'a str, protocol: &'a str) -> TopicContext<'a> {
    TopicContext {
        device_key,
        node_id: device_key,
        device_id: device_key,
        protocol,
        ..Default::default()
    }
}

/// Check if a node_id matches a node_filter string.
/// Filter is comma-separated prefixes. Empty filter matches everything.
pub fn matches_node_filter(node_id: &str, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    filter
        .split(',')
        .map(|s| s.trim())
        .any(|prefix| !prefix.is_empty() && node_id.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_value_topic() {
        let ctx = context_from_value("ahu-1/dat");
        let topic = resolve_topic(DEFAULT_VALUE_PATTERN, &ctx);
        assert_eq!(topic, "opencrate/ahu-1/dat/value");
    }

    #[test]
    fn resolve_alarm_topic() {
        let ctx = context_from_alarm("vav-1/zat", "");
        let topic = resolve_topic(DEFAULT_ALARM_PATTERN, &ctx);
        assert_eq!(topic, "opencrate/alarms/vav-1/zat");
    }

    #[test]
    fn resolve_status_topic() {
        let ctx = context_from_status("bacnet-1000", "bacnet");
        let topic = resolve_topic(DEFAULT_STATUS_PATTERN, &ctx);
        assert_eq!(topic, "opencrate/status/bacnet-1000");
    }

    #[test]
    fn unknown_variable_replaced() {
        let ctx = TopicContext::default();
        let topic = resolve_topic("test/{unknown_var}/end", &ctx);
        assert_eq!(topic, "test/_unknown_/end");
    }

    #[test]
    fn empty_value_becomes_underscore() {
        let ctx = TopicContext::default();
        let topic = resolve_topic("test/{device_id}/end", &ctx);
        assert_eq!(topic, "test/_/end");
    }

    #[test]
    fn sanitize_strips_wildcards() {
        let ctx = TopicContext {
            device_id: "dev+1#bad",
            ..Default::default()
        };
        let topic = resolve_topic("{device_id}", &ctx);
        assert_eq!(topic, "dev1bad");
    }

    #[test]
    fn sanitize_replaces_slashes() {
        let ctx = TopicContext {
            node_id: "ahu-1/dat",
            ..Default::default()
        };
        // node_id contains slash — should become dash in topic segment
        let topic = resolve_topic("prefix/{node_id}/suffix", &ctx);
        assert_eq!(topic, "prefix/ahu-1-dat/suffix");
    }

    #[test]
    fn node_filter_empty_matches_all() {
        assert!(matches_node_filter("anything", ""));
    }

    #[test]
    fn node_filter_prefix_match() {
        assert!(matches_node_filter("ahu-1/dat", "ahu-1"));
        assert!(!matches_node_filter("vav-1/zat", "ahu-1"));
    }

    #[test]
    fn node_filter_multiple_prefixes() {
        assert!(matches_node_filter("ahu-1/dat", "vav-1, ahu-1"));
        assert!(matches_node_filter("vav-1/zat", "vav-1, ahu-1"));
        assert!(!matches_node_filter("chiller-1/cwt", "vav-1, ahu-1"));
    }
}
