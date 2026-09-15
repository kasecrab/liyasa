//! A map that keeps the document's order.
//!
//! Properties, parameters, responses, and media types are rendered in the
//! order the spec author wrote them, so the model may not use a `BTreeMap`.
//! Spec maps are small — tens of entries, not thousands — so a vector with a
//! linear lookup is the right shape.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq)]
pub struct OrderedMap<V>(Vec<(String, V)>);

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<V> OrderedMap<V> {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.0.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    /// Appends, or replaces in place so a later override keeps the original
    /// position. Returns the value that was there.
    pub fn insert(&mut self, key: impl Into<String>, value: V) -> Option<V> {
        let key = key.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => Some(std::mem::replace(&mut slot.1, value)),
            None => {
                self.0.push((key, value));
                None
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<V> {
        let at = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(at).1)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&str, &mut V) -> bool) {
        self.0.retain_mut(|(k, v)| keep(k, v));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut V)> {
        self.0.iter_mut().map(|(k, v)| (k.as_str(), v))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(k, _)| k.as_str())
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<V> FromIterator<(String, V)> for OrderedMap<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(iter: I) -> Self {
        let mut out = Self::new();
        for (key, value) in iter {
            out.insert(key, value);
        }
        out
    }
}

impl<V> IntoIterator for OrderedMap<V> {
    type Item = (String, V);
    type IntoIter = std::vec::IntoIter<(String, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a OrderedMap<V> {
    type Item = (&'a str, &'a V);
    type IntoIter = Box<dyn Iterator<Item = (&'a str, &'a V)> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl<V: Serialize> Serialize for OrderedMap<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> OrderedMap<u8> {
        [("b", 1), ("a", 2), ("c", 3)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect()
    }

    #[test]
    fn iteration_is_insertion_order_not_sorted() {
        assert_eq!(fixture().keys().collect::<Vec<_>>(), vec!["b", "a", "c"]);
    }

    #[test]
    fn replacing_a_key_keeps_its_position() {
        let mut map = fixture();
        assert_eq!(map.insert("b", 9), Some(1));
        assert_eq!(map.keys().collect::<Vec<_>>(), vec!["b", "a", "c"]);
        assert_eq!(map.get("b"), Some(&9));
    }

    #[test]
    fn removing_shifts_rather_than_swaps() {
        let mut map = fixture();
        assert_eq!(map.remove("a"), Some(2));
        assert_eq!(map.keys().collect::<Vec<_>>(), vec!["b", "c"]);
        assert_eq!(map.remove("a"), None);
    }

    #[test]
    fn serializing_writes_the_documents_order() {
        let json = serde_json::to_string(&fixture()).expect("an ordered map serializes");
        assert_eq!(json, r#"{"b":1,"a":2,"c":3}"#);
    }
}
