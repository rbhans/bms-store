//! Pattern detection and sequential naming for device groups.
//!
//! Given one or more example device names (e.g. "VAV-101", "VAV-102"),
//! detects the naming pattern and generates sequential names for remaining devices.

/// A detected naming pattern that can generate sequential names.
#[derive(Debug, Clone, PartialEq)]
pub struct NamingPattern {
    pub prefix: String,
    pub start_number: u64,
    pub increment: u64,
    /// Zero-pad width. 0 = no padding, 2 = "01", 3 = "001".
    pub zero_pad: usize,
    pub suffix: String,
}

/// Detect a naming pattern from one or more ordered examples.
///
/// - 1 example: splits on last numeric segment → prefix + number + suffix, increment=1
/// - 2+ examples: detects common prefix, extracts numbers, computes increment, detects zero-padding
/// - No numbers found: returns None
pub fn detect_pattern(examples: &[&str]) -> Option<NamingPattern> {
    if examples.is_empty() {
        return None;
    }

    if examples.len() == 1 {
        return detect_single(examples[0]);
    }

    // Multi-example: find the pattern from the first two, validate with rest
    let first = parse_name_parts(examples[0])?;
    let second = parse_name_parts(examples[1])?;

    // Prefixes and suffixes must match
    if first.prefix != second.prefix || first.suffix != second.suffix {
        // Try single-example fallback from last example
        return detect_single(examples[examples.len() - 1]);
    }

    let increment = second.number.saturating_sub(first.number).max(1);
    let zero_pad = first.zero_pad.max(second.zero_pad);

    // The "next" number after all examples
    let last_number = first.number + (examples.len() as u64 - 1) * increment;

    Some(NamingPattern {
        prefix: first.prefix,
        start_number: last_number + increment,
        increment,
        zero_pad,
        suffix: first.suffix,
    })
}

/// Generate the next `count` names from a pattern.
pub fn generate_names(pattern: &NamingPattern, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            let num = pattern.start_number + (i as u64) * pattern.increment;
            let num_str = if pattern.zero_pad > 0 {
                format!("{:0>width$}", num, width = pattern.zero_pad)
            } else {
                num.to_string()
            };
            format!("{}{}{}", pattern.prefix, num_str, pattern.suffix)
        })
        .collect()
}

/// Find & replace across a list of names.
pub fn find_replace(names: &[String], find: &str, replace: &str) -> Vec<String> {
    names.iter().map(|n| n.replace(find, replace)).collect()
}

/// Add prefix and/or suffix to all names.
pub fn add_prefix_suffix(names: &[String], prefix: &str, suffix: &str) -> Vec<String> {
    names
        .iter()
        .map(|n| format!("{}{}{}", prefix, n, suffix))
        .collect()
}

/// Context for template-based naming.
pub struct TemplateContext {
    pub device: String,
    pub point: String,
    pub kind: String,
    pub units: String,
    pub index: usize,
    pub protocol: String,
    pub network: String,
}

/// Apply a template string with placeholders:
/// `{device}`, `{point}`, `{kind}`, `{units}`, `{index}`, `{protocol}`, `{network}`
pub fn apply_template(template: &str, ctx: &TemplateContext) -> String {
    template
        .replace("{device}", &ctx.device)
        .replace("{point}", &ctx.point)
        .replace("{kind}", &ctx.kind)
        .replace("{units}", &ctx.units)
        .replace("{index}", &ctx.index.to_string())
        .replace("{protocol}", &ctx.protocol)
        .replace("{network}", &ctx.network)
}

/// Apply a template to multiple contexts, generating a name per context.
pub fn apply_template_batch(template: &str, contexts: &[TemplateContext]) -> Vec<String> {
    contexts
        .iter()
        .map(|ctx| apply_template(template, ctx))
        .collect()
}

// ── Internal helpers ──

struct NameParts {
    prefix: String,
    number: u64,
    zero_pad: usize,
    suffix: String,
}

/// Parse a name by splitting on the last numeric segment.
fn parse_name_parts(name: &str) -> Option<NameParts> {
    // Find the last contiguous run of digits
    let chars: Vec<char> = name.chars().collect();
    let mut num_end = None;
    let mut num_start = None;

    // Scan backwards to find the last digit run
    for i in (0..chars.len()).rev() {
        if chars[i].is_ascii_digit() {
            if num_end.is_none() {
                num_end = Some(i + 1);
            }
            num_start = Some(i);
        } else if num_end.is_some() {
            break;
        }
    }

    let start = num_start?;
    let end = num_end?;

    let prefix = &name[..start];
    let num_str = &name[start..end];
    let suffix = &name[end..];

    let number: u64 = num_str.parse().ok()?;
    let zero_pad = if num_str.starts_with('0') && num_str.len() > 1 {
        num_str.len()
    } else {
        0
    };

    Some(NameParts {
        prefix: prefix.to_string(),
        number,
        zero_pad,
        suffix: suffix.to_string(),
    })
}

fn detect_single(name: &str) -> Option<NamingPattern> {
    let parts = parse_name_parts(name)?;
    Some(NamingPattern {
        prefix: parts.prefix,
        start_number: parts.number + 1,
        increment: 1,
        zero_pad: parts.zero_pad,
        suffix: parts.suffix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_single_example() {
        let pat = detect_pattern(&["VAV-101"]).unwrap();
        assert_eq!(pat.prefix, "VAV-");
        assert_eq!(pat.start_number, 102);
        assert_eq!(pat.increment, 1);
        assert_eq!(pat.zero_pad, 0);
        assert_eq!(pat.suffix, "");
    }

    #[test]
    fn detect_two_examples() {
        let pat = detect_pattern(&["VAV-101", "VAV-102"]).unwrap();
        assert_eq!(pat.prefix, "VAV-");
        assert_eq!(pat.start_number, 103);
        assert_eq!(pat.increment, 1);
        assert_eq!(pat.zero_pad, 0);
        assert_eq!(pat.suffix, "");
    }

    #[test]
    fn detect_two_examples_with_increment() {
        let pat = detect_pattern(&["AHU-10", "AHU-20"]).unwrap();
        assert_eq!(pat.prefix, "AHU-");
        assert_eq!(pat.start_number, 30);
        assert_eq!(pat.increment, 10);
    }

    #[test]
    fn detect_zero_padded() {
        let pat = detect_pattern(&["RTU-001", "RTU-002"]).unwrap();
        assert_eq!(pat.prefix, "RTU-");
        assert_eq!(pat.start_number, 3);
        assert_eq!(pat.increment, 1);
        assert_eq!(pat.zero_pad, 3);
    }

    #[test]
    fn detect_with_suffix() {
        let pat = detect_pattern(&["Floor1-VAV-01A"]).unwrap();
        assert_eq!(pat.prefix, "Floor1-VAV-");
        assert_eq!(pat.start_number, 2);
        assert_eq!(pat.zero_pad, 2);
        assert_eq!(pat.suffix, "A");
    }

    #[test]
    fn detect_no_numbers_returns_none() {
        assert!(detect_pattern(&["VAV"]).is_none());
        assert!(detect_pattern(&["AHU-North"]).is_none());
    }

    #[test]
    fn generate_names_basic() {
        let pat = NamingPattern {
            prefix: "VAV-".into(),
            start_number: 103,
            increment: 1,
            zero_pad: 0,
            suffix: String::new(),
        };
        let names = generate_names(&pat, 3);
        assert_eq!(names, vec!["VAV-103", "VAV-104", "VAV-105"]);
    }

    #[test]
    fn generate_names_zero_padded() {
        let pat = NamingPattern {
            prefix: "RTU-".into(),
            start_number: 3,
            increment: 1,
            zero_pad: 3,
            suffix: String::new(),
        };
        let names = generate_names(&pat, 2);
        assert_eq!(names, vec!["RTU-003", "RTU-004"]);
    }

    #[test]
    fn generate_names_with_increment() {
        let pat = NamingPattern {
            prefix: "AHU-".into(),
            start_number: 30,
            increment: 10,
            zero_pad: 0,
            suffix: String::new(),
        };
        let names = generate_names(&pat, 3);
        assert_eq!(names, vec!["AHU-30", "AHU-40", "AHU-50"]);
    }

    #[test]
    fn find_replace_basic() {
        let names = vec!["VAV-101".into(), "VAV-102".into()];
        let result = find_replace(&names, "VAV", "FCU");
        assert_eq!(result, vec!["FCU-101", "FCU-102"]);
    }

    #[test]
    fn add_prefix_suffix_basic() {
        let names = vec!["VAV-1".into(), "VAV-2".into()];
        let result = add_prefix_suffix(&names, "B1-", " Zone");
        assert_eq!(result, vec!["B1-VAV-1 Zone", "B1-VAV-2 Zone"]);
    }

    #[test]
    fn detect_three_examples() {
        let pat = detect_pattern(&["FCU-1", "FCU-2", "FCU-3"]).unwrap();
        assert_eq!(pat.prefix, "FCU-");
        assert_eq!(pat.start_number, 4);
        assert_eq!(pat.increment, 1);
    }

    #[test]
    fn empty_examples_returns_none() {
        assert!(detect_pattern(&[]).is_none());
    }

    #[test]
    fn template_basic() {
        let ctx = TemplateContext {
            device: "AHU-1".into(),
            point: "ZoneTemp".into(),
            kind: "sensor".into(),
            units: "degF".into(),
            index: 0,
            protocol: "bacnet".into(),
            network: "ip-main".into(),
        };
        assert_eq!(apply_template("{device}/{point}", &ctx), "AHU-1/ZoneTemp");
        assert_eq!(
            apply_template("{protocol}-{device}-{index}", &ctx),
            "bacnet-AHU-1-0"
        );
    }

    #[test]
    fn template_batch() {
        let contexts = vec![
            TemplateContext {
                device: "VAV-1".into(),
                point: "Damper".into(),
                kind: "output".into(),
                units: "%".into(),
                index: 0,
                protocol: "bacnet".into(),
                network: "default".into(),
            },
            TemplateContext {
                device: "VAV-2".into(),
                point: "Damper".into(),
                kind: "output".into(),
                units: "%".into(),
                index: 1,
                protocol: "bacnet".into(),
                network: "default".into(),
            },
        ];
        let names = apply_template_batch("{device}-{point}", &contexts);
        assert_eq!(names, vec!["VAV-1-Damper", "VAV-2-Damper"]);
    }
}
