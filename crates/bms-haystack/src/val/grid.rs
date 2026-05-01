use super::dict::Dict;

/// Grid column metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Col {
    pub name: String,
    /// Per-column meta tags (units, dis, etc.). May be empty.
    pub meta: Dict,
}

impl Col {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            meta: Dict::default(),
        }
    }

    pub fn with_meta(name: impl Into<String>, meta: Dict) -> Self {
        Self {
            name: name.into(),
            meta,
        }
    }
}

/// Haystack `Grid` — two-dimensional table of columns and row dicts plus
/// a grid-level meta block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Grid {
    pub meta: Dict,
    pub cols: Vec<Col>,
    pub rows: Vec<Dict>,
}

impl Grid {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_rows(rows: Vec<Dict>) -> Self {
        let cols = derive_cols(&rows);
        Self {
            meta: Dict::default(),
            cols,
            rows,
        }
    }

    pub fn with_meta(mut self, meta: Dict) -> Self {
        self.meta = meta;
        self
    }

    pub fn ver(&self) -> &str {
        // Haystack 4 conformance: the grid carries `ver` in meta, default "3.0".
        // For Haystack 5 (Hayson) the version field is implicit — left blank.
        match self.meta.get("ver") {
            Some(super::value::Value::Str(s)) => s.as_str(),
            _ => "3.0",
        }
    }
}

fn derive_cols(rows: &[Dict]) -> Vec<Col> {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for row in rows {
        for (k, _) in row.iter() {
            seen.insert(k.as_str());
        }
    }
    seen.into_iter().map(Col::new).collect()
}
