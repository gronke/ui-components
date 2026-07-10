//! The object-valued map behind `Value::Object` — a plain JS object analog
//! for state-shaped properties (ADR 0013).

use std::collections::BTreeMap;

use crate::value::Value;

/// A string-keyed value map with deterministic (sorted) key order, so its
/// serializations compare bytewise across producers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObjectMap(BTreeMap<String, Value>);

impl ObjectMap {
    pub fn new() -> Self {
        ObjectMap::default()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.0.insert(key.into(), value.into());
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.0.remove(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for ObjectMap {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(entries: I) -> Self {
        ObjectMap(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_iterate_sorted_regardless_of_insertion_order() {
        let mut map = ObjectMap::new();
        map.insert("zone", "UTC");
        map.insert("date", "2026-07-07");
        let keys: Vec<&str> = map.iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["date", "zone"]);
    }

    #[test]
    fn equality_ignores_insertion_order() {
        let a: ObjectMap = [("a", 1.0), ("b", 2.0)].into_iter().collect();
        let b: ObjectMap = [("b", 2.0), ("a", 1.0)].into_iter().collect();
        assert_eq!(a, b);
    }
}
