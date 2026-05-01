//! Build-time codegen — parses the vendored Project Haystack 5 / Xeto
//! libraries under `assets/xeto-master/src/xeto/` and emits Rust source
//! tables of types and global tags into `OUT_DIR`.
//!
//! The parser is a hand-rolled, line-oriented scanner with bracket-depth
//! tracking. It captures: type name, supertype expression, meta block
//! (`<key:val, flag>`), default value, doc, and (for `*name: Kind` global
//! slots inside a body) the same plus inferred kind. It does NOT understand
//! every xeto construct — `Query<...>`, complex intersection types, and
//! enum bodies are treated opaquely. Coverage gaps surface as parser
//! warnings (printed via `cargo:warning=`).

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let xeto_root = manifest_dir.join("../../assets/xeto-master/src/xeto");
    println!("cargo:rerun-if-changed={}", xeto_root.display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut specs: Vec<Spec> = Vec::new();
    let mut globals: Vec<Global> = Vec::new();
    let mut warn_count = 0usize;

    if !xeto_root.exists() {
        println!(
            "cargo:warning=xeto bundle missing at {} — skipping codegen",
            xeto_root.display()
        );
        emit_empty(&out_path());
        return;
    }

    for lib_dir in collect_lib_dirs(&xeto_root) {
        let lib_name = lib_dir.file_name().unwrap().to_string_lossy().to_string();
        for xeto_file in collect_xeto_files(&lib_dir) {
            let content = match fs::read_to_string(&xeto_file) {
                Ok(c) => c,
                Err(e) => {
                    println!(
                        "cargo:warning=failed reading {}: {}",
                        xeto_file.display(),
                        e
                    );
                    warn_count += 1;
                    continue;
                }
            };
            parse_file(&content, &lib_name, &mut specs, &mut globals, &mut warn_count);
        }
    }

    specs.sort_by(|a, b| (a.lib.clone(), a.name.clone()).cmp(&(b.lib.clone(), b.name.clone())));
    globals.sort_by(|a, b| a.name.cmp(&b.name));
    globals.dedup_by(|a, b| a.name == b.name);

    let out = out_path();
    emit_generated(&out, &specs, &globals);

    println!(
        "cargo:warning=bms-haystack codegen: {} specs, {} globals, {} warnings",
        specs.len(),
        globals.len(),
        warn_count
    );
}

fn out_path() -> PathBuf {
    PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set"))
}

fn collect_lib_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn collect_xeto_files(lib_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(lib_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("xeto") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[derive(Clone, Default)]
struct MetaBlock {
    /// Bare flags like `abstract`, `sealed`, `unitless`, `nodoc`, `invariant`.
    flags: Vec<String>,
    /// `quantity:"temperature"` — numeric quantity.
    quantity: Option<String>,
    /// `of:Equip` — reference target type.
    of_type: Option<String>,
    /// `unit:"degF"` — unit code (only on Number-kind specs).
    unit: Option<String>,
    /// `pattern:"..."` — regex pattern.
    pattern: Option<String>,
    /// `version:"..."`.
    version: Option<String>,
}

#[derive(Clone)]
struct Spec {
    name: String,
    supertype: String,
    meta: MetaBlock,
    lib: String,
    doc: String,
    /// Default scalar string after the meta block (e.g. `"degF"`, `"0"`, `"✓"`).
    default_val: Option<String>,
}

#[derive(Clone)]
struct Global {
    name: String,
    kind: String,
    meta: MetaBlock,
    lib: String,
    doc: String,
}

fn parse_file(
    content: &str,
    lib_name: &str,
    specs: &mut Vec<Spec>,
    globals: &mut Vec<Global>,
    warn_count: &mut usize,
) {
    let mut pending_doc = String::new();
    let mut depth_curly: i32 = 0;
    let mut depth_angle: i32 = 0;
    let mut in_string = false;
    let mut prev = '\0';

    let lines: Vec<&str> = content.split('\n').collect();
    for line in lines {
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
        let mut produced_decl_this_line = false;

        // Update depth from this line's chars (respecting strings + comments).
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
            if let Some(decl) = match_top_decl(line) {
                let doc = std::mem::take(&mut pending_doc);
                specs.push(Spec {
                    name: decl.name,
                    supertype: decl.supertype,
                    meta: decl.meta,
                    lib: lib_name.to_string(),
                    doc: clean_doc(&doc),
                    default_val: decl.default_val,
                });
                produced_decl_this_line = true;
            }
        }

        if line_starts_at_depth >= 1 {
            if let Some(slot) = match_global_slot(trimmed) {
                let doc = std::mem::take(&mut pending_doc);
                globals.push(Global {
                    name: slot.name,
                    kind: slot.kind,
                    meta: slot.meta,
                    lib: lib_name.to_string(),
                    doc: clean_doc(&doc),
                });
            }
        }

        if !produced_decl_this_line && line_starts_at_depth == 0 {
            pending_doc.clear();
        }
    }

    if depth_curly != 0 || depth_angle != 0 {
        *warn_count += 1;
    }
}

struct TopDecl {
    name: String,
    supertype: String,
    meta: MetaBlock,
    default_val: Option<String>,
}

struct SlotDecl {
    name: String,
    kind: String,
    meta: MetaBlock,
}

/// Parse `Name : SuperType <meta> "default"` style top-level declarations.
fn match_top_decl(line: &str) -> Option<TopDecl> {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0] as char;
    if !first.is_ascii_uppercase() {
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
    let rest = line[i..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();

    // Take supertype expression up to '<', '{', '"', or end-of-line / comment.
    let supertype_end = rest
        .find(|c: char| c == '<' || c == '{' || c == '"' || c == '\r' || c == '\n')
        .unwrap_or(rest.len());
    let supertype = rest[..supertype_end].trim().to_string();
    if supertype.is_empty() {
        return None;
    }
    let after_super = rest[supertype_end..].trim_start();

    // Optional meta block: skip if ambiguous (multi-line)
    let (meta, after_meta) = take_meta(after_super);

    // Optional default value: a `"..."` literal after meta.
    let default_val = parse_default(after_meta);

    Some(TopDecl {
        name,
        supertype,
        meta,
        default_val,
    })
}

/// Parse `*name: Kind <meta> "default"` global slot lines.
fn match_global_slot(trimmed: &str) -> Option<SlotDecl> {
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
    let rest = after_star[i..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
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
    let after_kind = rest[k..].trim_start();
    let after_kind = after_kind.strip_prefix('?').unwrap_or(after_kind).trim_start();
    let (meta, _after_meta) = take_meta(after_kind);
    Some(SlotDecl { name, kind, meta })
}

/// If `s` starts with `<...>`, parse and return (MetaBlock, rest).
/// Otherwise returns (empty MetaBlock, s).
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
        return (MetaBlock::default(), s); // malformed — give up
    };
    let inner = &s[1..close_idx];
    let meta = parse_meta(inner);
    let rest = s[close_idx + 1..].trim_start();
    (meta, rest)
}

/// Parse comma-separated `key:val` or bare-flag entries inside a `<...>` block.
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
                "pattern" => out.pattern = Some(val.to_string()),
                "version" => out.version = Some(val.to_string()),
                _ => {} // ignore unknown keys for now
            }
        } else {
            // bare flag
            out.flags.push(entry.to_string());
        }
    }
    out
}

/// Comma-split that respects nested `< >`, `{ }`, and `"..."`.
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
    // Take through closing `"` (no escape handling beyond basic)
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

fn emit_empty(out_dir: &Path) {
    let path = out_dir.join("generated.rs");
    let mut f = fs::File::create(&path).expect("create generated.rs");
    writeln!(f, "// Auto-generated by build.rs (xeto bundle missing).").unwrap();
    writeln!(f, "pub static GENERATED_SPECS: &[GeneratedSpec] = &[];").unwrap();
    writeln!(f, "pub static GENERATED_GLOBALS: &[GeneratedGlobal] = &[];").unwrap();
}

fn emit_generated(out_dir: &Path, specs: &[Spec], globals: &[Global]) {
    let path = out_dir.join("generated.rs");
    let mut f = fs::File::create(&path).expect("create generated.rs");
    writeln!(f, "// Auto-generated by build.rs from assets/xeto-master/.").unwrap();
    writeln!(f, "// Do not edit; regeneration runs on every change to that tree.").unwrap();
    writeln!(f).unwrap();

    writeln!(f, "pub static GENERATED_SPECS: &[GeneratedSpec] = &[").unwrap();
    for s in specs {
        let m = &s.meta;
        writeln!(
            f,
            "    GeneratedSpec {{ name: {n}, supertype: {st}, lib: {l}, doc: {d}, abstract_: {a}, sealed: {se}, of_type: {of}, quantity: {q}, unit: {u}, default_val: {dv} }},",
            n = rust_str(&s.name),
            st = rust_str(&s.supertype),
            l = rust_str(&s.lib),
            d = rust_str(&s.doc),
            a = if m.flags.iter().any(|x| x == "abstract") { "true" } else { "false" },
            se = if m.flags.iter().any(|x| x == "sealed") { "true" } else { "false" },
            of = rust_opt(m.of_type.as_deref()),
            q = rust_opt(m.quantity.as_deref()),
            u = rust_opt(m.unit.as_deref()),
            dv = rust_opt(s.default_val.as_deref()),
        )
        .unwrap();
    }
    writeln!(f, "];").unwrap();
    writeln!(f).unwrap();

    writeln!(f, "pub static GENERATED_GLOBALS: &[GeneratedGlobal] = &[").unwrap();
    for g in globals {
        let m = &g.meta;
        writeln!(
            f,
            "    GeneratedGlobal {{ name: {n}, kind: {k}, lib: {l}, doc: {d}, of_type: {of}, quantity: {q}, unit: {u} }},",
            n = rust_str(&g.name),
            k = rust_str(&g.kind),
            l = rust_str(&g.lib),
            d = rust_str(&g.doc),
            of = rust_opt(m.of_type.as_deref()),
            q = rust_opt(m.quantity.as_deref()),
            u = rust_opt(m.unit.as_deref()),
        )
        .unwrap();
    }
    writeln!(f, "];").unwrap();
}

fn rust_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn rust_opt(opt: Option<&str>) -> String {
    match opt {
        Some(s) => format!("Some({})", rust_str(s)),
        None => "None".to_string(),
    }
}
