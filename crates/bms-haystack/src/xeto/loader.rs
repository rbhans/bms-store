//! Runtime xeto namespace loader.
//!
//! Walks a directory of xeto libraries (each lib in its own subdirectory
//! containing `lib.xeto` plus other `.xeto` files) and merges what it
//! parses into a [`HaystackNamespace`]. The build-time generated tables
//! (`GENERATED_SPECS` / `GENERATED_GLOBALS`) are also folded in by default
//! so the final namespace is a union of "shipped" + "user-supplied" specs.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use super::parser::{parse_lib_dir, ParseError, RuntimeGlobal, RuntimeSpec};
use crate::ontology::{GeneratedGlobal, GeneratedSpec, GENERATED_GLOBALS, GENERATED_SPECS};

/// Merged catalog of build-time + runtime ontology data.
///
/// `specs` and `globals` are deduplicated by `name` — runtime entries
/// override build-time entries with the same name, which is the
/// expected behavior for vendor-supplied extensions.
#[derive(Debug, Default)]
pub struct HaystackNamespace {
    specs: Vec<RuntimeSpec>,
    globals: Vec<RuntimeGlobal>,
    /// Source directories last loaded — drives `reload`.
    sources: Vec<PathBuf>,
}

impl HaystackNamespace {
    /// Build from the build-time generated tables only — useful in tests
    /// and when no runtime libs are deployed.
    pub fn builtin() -> Self {
        let mut ns = Self::default();
        ns.merge_builtin();
        ns
    }

    /// Load every lib subdirectory under `root`, fold in build-time data,
    /// and return the merged namespace.
    pub fn load(root: &Path) -> Result<Self, ParseError> {
        let mut ns = Self::default();
        ns.merge_builtin();
        ns.load_into(root)?;
        ns.sources.push(root.to_path_buf());
        Ok(ns)
    }

    /// Reload from the original source directories. Useful from a SIGHUP
    /// handler — a fresh namespace replaces the live one atomically via
    /// the [`SharedNamespace`] wrapper.
    pub fn reload(&self) -> Result<Self, ParseError> {
        let mut ns = Self::default();
        ns.merge_builtin();
        for src in &self.sources {
            ns.load_into(src)?;
        }
        ns.sources = self.sources.clone();
        Ok(ns)
    }

    fn merge_builtin(&mut self) {
        for s in GENERATED_SPECS {
            self.specs.push(spec_from_generated(s));
        }
        for g in GENERATED_GLOBALS {
            self.globals.push(global_from_generated(g));
        }
    }

    fn load_into(&mut self, root: &Path) -> Result<(), ParseError> {
        if !root.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let lib_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let parsed = parse_lib_dir(&path, &lib_name)?;
            // Replace existing entries with the same (lib, name) coordinates.
            for new_spec in parsed.specs {
                self.specs
                    .retain(|s| !(s.name == new_spec.name && s.lib == new_spec.lib));
                self.specs.push(new_spec);
            }
            for new_global in parsed.globals {
                self.globals
                    .retain(|g| !(g.name == new_global.name && g.lib == new_global.lib));
                self.globals.push(new_global);
            }
        }
        Ok(())
    }

    pub fn specs(&self) -> &[RuntimeSpec] {
        &self.specs
    }

    pub fn globals(&self) -> &[RuntimeGlobal] {
        &self.globals
    }

    pub fn find_spec(&self, name: &str) -> Option<&RuntimeSpec> {
        self.specs.iter().find(|s| s.name == name)
    }

    pub fn find_global(&self, name: &str) -> Option<&RuntimeGlobal> {
        self.globals.iter().find(|g| g.name == name)
    }
}

fn spec_from_generated(g: &GeneratedSpec) -> RuntimeSpec {
    RuntimeSpec {
        name: g.name.to_string(),
        supertype: g.supertype.to_string(),
        lib: g.lib.to_string(),
        doc: g.doc.to_string(),
        abstract_: g.abstract_,
        sealed: g.sealed,
        of_type: g.of_type.map(String::from),
        quantity: g.quantity.map(String::from),
        unit: g.unit.map(String::from),
        default_val: g.default_val.map(String::from),
    }
}

fn global_from_generated(g: &GeneratedGlobal) -> RuntimeGlobal {
    RuntimeGlobal {
        name: g.name.to_string(),
        kind: g.kind.to_string(),
        lib: g.lib.to_string(),
        doc: g.doc.to_string(),
        of_type: g.of_type.map(String::from),
        quantity: g.quantity.map(String::from),
        unit: g.unit.map(String::from),
    }
}

/// Atomically-swappable namespace handle. Hold an `Arc<SharedNamespace>`
/// in your service state; on SIGHUP call `swap_in(reloaded)`.
pub struct SharedNamespace {
    inner: RwLock<Arc<HaystackNamespace>>,
}

impl SharedNamespace {
    pub fn new(ns: HaystackNamespace) -> Self {
        Self {
            inner: RwLock::new(Arc::new(ns)),
        }
    }

    pub fn current(&self) -> Arc<HaystackNamespace> {
        self.inner.read().unwrap().clone()
    }

    pub fn swap_in(&self, ns: HaystackNamespace) {
        *self.inner.write().unwrap() = Arc::new(ns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn builtin_includes_upstream_specs() {
        let ns = HaystackNamespace::builtin();
        assert!(ns.specs().len() > 500);
        assert!(ns.find_spec("Site").is_some());
        assert!(ns.find_spec("Equip").is_some());
    }

    #[test]
    fn loads_user_lib_overlay() {
        let dir = tempdir();
        let lib_dir = dir.join("acme");
        fs::create_dir_all(&lib_dir).unwrap();
        let mut f = fs::File::create(lib_dir.join("lib.xeto")).unwrap();
        writeln!(
            f,
            "// Acme custom chiller spec\nAcmeChiller: Chiller <sealed>\n"
        )
        .unwrap();

        let ns = HaystackNamespace::load(&dir).unwrap();
        let s = ns.find_spec("AcmeChiller").expect("AcmeChiller missing");
        assert_eq!(s.lib, "acme");
        assert_eq!(s.supertype, "Chiller");
        assert!(s.sealed);
    }

    #[test]
    fn shared_namespace_swap() {
        let a = HaystackNamespace::builtin();
        let baseline = a.specs().len();
        let shared = SharedNamespace::new(a);
        let cur = shared.current();
        assert_eq!(cur.specs().len(), baseline);
        shared.swap_in(HaystackNamespace::builtin());
        assert_eq!(shared.current().specs().len(), baseline);
    }

    fn tempdir() -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("bms-haystack-test-{pid}-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
