//! Runtime xeto parser. Mirrors the build-time parser in `build.rs` but
//! returns owned records suitable for in-memory use. The two parsers are
//! deliberately kept in sync; if you fix a bug in one, fix it in the other.
//!
//! Scope: enough to extract type names, supertypes, kinds, doc strings,
//! and meta (`<key:val, flag>`). Slot bodies and complex query inversions
//! are skipped.

use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Bag of parsed records from one or more `.xeto` files.
#[derive(Debug, Clone, Default)]
pub struct ParsedLib {
    pub specs: Vec<RuntimeSpec>,
    pub globals: Vec<RuntimeGlobal>,
}

/// Top-level type declaration captured from xeto source.
#[derive(Debug, Clone)]
pub struct RuntimeSpec {
    pub name: String,
    pub supertype: String,
    pub lib: String,
    pub doc: String,
    pub abstract_: bool,
    pub sealed: bool,
    pub of_type: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub default_val: Option<String>,
}

/// `*name: Kind` global slot captured from a PhEntity-style body.
#[derive(Debug, Clone)]
pub struct RuntimeGlobal {
    pub name: String,
    pub kind: String,
    pub lib: String,
    pub doc: String,
    pub of_type: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct MetaBlock {
    flags: Vec<String>,
    quantity: Option<String>,
    of_type: Option<String>,
    unit: Option<String>,
}

/// Parse one xeto source file into specs + globals.
pub fn parse_source(content: &str, lib_name: &str) -> ParsedLib {
    let mut out = ParsedLib::default();
    let mut pending_doc = String::new();
    let mut depth_curly: i32 = 0;
    let mut depth_angle: i32 = 0;
    let mut in_string = false;
    let mut prev = '\0';

    for line in content.split('\n') {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("//") {
            if depth_curly == 0 && depth_angle == 0 {
                let r = rest.trim_start_matches(' ').trim_start_matches('/');
                if !pending_doc.is_empty() {
                    pending_doc.push('\n');
                }
                pending_doc.push_str(r.trim_end());
            }
            continue;
        }
        if trimmed.is_empty() {
            if depth_curly == 0 && depth_angle == 0 {
                pending_doc.clear();
            }
            continue;
        }

        let line_starts_at_depth = depth_curly;

        // Update bracket depth from this line.
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if !in_string && c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            if c == '"' && prev != '\\' {
                in_string = !in_string;
            }
            if !in_string {
                match c {
                    '{' => depth_curly += 1,
                    '}' => depth_curly -= 1,
                    '<' => depth_angle += 1,
                    '>' => depth_angle -= 1,
                    _ => {}
                }
            }
            prev = c;
        }
        if depth_curly < 0 {
            depth_curly = 0;
        }
        if depth_angle < 0 {
            depth_angle = 0;
        }

        if line_starts_at_depth == 0 && !line.starts_with(' ') && !line.starts_with('\t') {
            if let Some((name, supertype, meta, default_val)) = match_top_decl(line) {
                let doc = std::mem::take(&mut pending_doc);
                out.specs.push(RuntimeSpec {
                    name,
                    supertype,
                    lib: lib_name.to_string(),
                    doc: clean_doc(&doc),
                    abstract_: meta.flags.iter().any(|f| f == "abstract"),
                    sealed: meta.flags.iter().any(|f| f == "sealed"),
                    of_type: meta.of_type.clone(),
                    quantity: meta.quantity.clone(),
                    unit: meta.unit.clone(),
                    default_val,
                });
            }
        }

        if line_starts_at_depth >= 1 {
            if let Some((name, kind, meta)) = match_global_slot(trimmed) {
                let doc = std::mem::take(&mut pending_doc);
                out.globals.push(RuntimeGlobal {
                    name,
                    kind,
                    lib: lib_name.to_string(),
                    doc: clean_doc(&doc),
                    of_type: meta.of_type,
                    quantity: meta.quantity,
                    unit: meta.unit,
                });
            }
        }

        if line_starts_at_depth == 0 {
            pending_doc.clear();
        }
    }

    out
}

/// Parse all `*.xeto` files in a single library directory.
pub fn parse_lib_dir(dir: &Path, lib_name: &str) -> Result<ParsedLib, ParseError> {
    let mut out = ParsedLib::default();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("xeto") {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let mut parsed = parse_source(&content, lib_name);
        out.specs.append(&mut parsed.specs);
        out.globals.append(&mut parsed.globals);
    }
    Ok(out)
}

fn match_top_decl(line: &str) -> Option<(String, String, MetaBlock, Option<String>)> {
    let bytes = line.as_bytes();
    if bytes.is_empty() || !(bytes[0] as char).is_ascii_uppercase() {
        return None;
    }
    if line.starts_with("pragma") {
        return None;
    }
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            i += 1;
        } else {
            break;
        }
    }
    let name = line[..i].to_string();
    if name.is_empty() {
        return None;
    }
    let rest = line[i..].trim_start().strip_prefix(':')?.trim_start();
    let supertype_end = rest
        .find(|c: char| c == '<' || c == '{' || c == '"' || c == '\r' || c == '\n')
        .unwrap_or(rest.len());
    let supertype = rest[..supertype_end].trim().to_string();
    if supertype.is_empty() {
        return None;
    }
    let after_super = rest[supertype_end..].trim_start();
    let (meta, after_meta) = take_meta(after_super);
    let default_val = parse_default(after_meta);
    Some((name, supertype, meta, default_val))
}

fn match_global_slot(trimmed: &str) -> Option<(String, String, MetaBlock)> {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || bytes[0] != b'*' {
        return None;
    }
    let after_star = &trimmed[1..];
    let mut i = 0;
    let ab = after_star.as_bytes();
    while i < ab.len() {
        let c = ab[i] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            i += 1;
        } else {
            break;
        }
    }
    if i == 0 {
        return None;
    }
    let name = after_star[..i].to_string();
    let rest = after_star[i..].trim_start().strip_prefix(':')?.trim_start();
    let rb = rest.as_bytes();
    let mut k = 0;
    while k < rb.len() {
        let c = rb[k] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            k += 1;
        } else {
            break;
        }
    }
    if k == 0 {
        return None;
    }
    let kind = rest[..k].to_string();
    let after_kind = rest[k..].trim_start().trim_start_matches('?').trim_start();
    let (meta, _) = take_meta(after_kind);
    Some((name, kind, meta))
}

fn take_meta(s: &str) -> (MetaBlock, &str) {
    if !s.starts_with('<') {
        return (MetaBlock::default(), s);
    }
    let mut depth = 0i32;
    let mut end = None;
    let mut in_str = false;
    let mut prev = '\0';
    for (i, c) in s.char_indices() {
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        }
        if !in_str {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        prev = c;
    }
    let Some(close_idx) = end else {
        return (MetaBlock::default(), s);
    };
    let inner = &s[1..close_idx];
    (parse_meta(inner), s[close_idx + 1..].trim_start())
}

fn parse_meta(inner: &str) -> MetaBlock {
    let mut out = MetaBlock::default();
    for entry in split_meta_entries(inner) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if let Some((k, v)) = entry.split_once(':') {
            let key = k.trim();
            let val = strip_quotes(v.trim());
            match key {
                "quantity" => out.quantity = Some(val.to_string()),
                "of" => out.of_type = Some(val.to_string()),
                "unit" => out.unit = Some(val.to_string()),
                _ => {}
            }
        } else {
            out.flags.push(entry.to_string());
        }
    }
    out
}

fn split_meta_entries(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut prev = '\0';
    for c in s.chars() {
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        }
        if !in_str {
            match c {
                '<' | '{' | '[' | '(' => depth += 1,
                '>' | '}' | ']' | ')' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(std::mem::take(&mut buf));
                    prev = c;
                    continue;
                }
                _ => {}
            }
        }
        buf.push(c);
        prev = c;
    }
    if !buf.trim().is_empty() {
        out.push(buf);
    }
    out
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn parse_default(s: &str) -> Option<String> {
    let s = s.trim_start();
    if !s.starts_with('"') {
        return None;
    }
    let mut prev = '\0';
    for (i, c) in s.char_indices().skip(1) {
        if c == '"' && prev != '\\' {
            return Some(s[1..i].to_string());
        }
        prev = c;
    }
    None
}

fn clean_doc(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let first = trimmed.lines().next().unwrap_or("");
    first.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_marker_decl() {
        let src = "// A doc string\nFoo: Marker <abstract>\n";
        let p = parse_source(src, "test");
        assert_eq!(p.specs.len(), 1);
        let s = &p.specs[0];
        assert_eq!(s.name, "Foo");
        assert_eq!(s.supertype, "Marker");
        assert!(s.abstract_);
        assert_eq!(s.doc, "A doc string");
    }

    #[test]
    fn parses_global_slot() {
        let src = "PhEntity: Entity <abstract> {\n  *foo: Marker\n  *area: Number <quantity:\"area\">\n}\n";
        let p = parse_source(src, "ph");
        assert!(p.globals.iter().any(|g| g.name == "foo"));
        let area = p.globals.iter().find(|g| g.name == "area").unwrap();
        assert_eq!(area.kind, "Number");
        assert_eq!(area.quantity.as_deref(), Some("area"));
    }
}
