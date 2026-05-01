use std::collections::BTreeMap;

use super::value::Value;

/// Haystack `Dict` — ordered map of name → value.
///
/// Names follow xeto identifier rules (camelCase, leading lower-case letter).
/// We use [`BTreeMap`] so iteration order is stable across encodings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dict {
    pub tags: BTreeMap<String, Value>,
}

impl Dict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(name: impl Into<String>, val: impl Into<Value>) -> Self {
        let mut d = Self::default();
        d.insert(name, val);
        d
    }

    pub fn insert(&mut self, name: impl Into<String>, val: impl Into<Value>) -> &mut Self {
        self.tags.insert(name.into(), val.into());
        self
    }

    pub fn marker(&mut self, name: impl Into<String>) -> &mut Self {
        self.insert(name, Value::Marker)
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.tags.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.tags.contains_key(name)
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.tags.iter()
    }
}

impl<K, V> FromIterator<(K, V)> for Dict
where
    K: Into<String>,
    V: Into<Value>,
{
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self {
            tags: iter.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
        }
    }
}
