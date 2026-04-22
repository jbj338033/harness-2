// IMPLEMENTS: D-449
//! The two CRDT types we ship: `LwwRegister` (last-write-wins
//! register) and `OrSet` (observed-removed set). Anything more
//! ambitious — automerge / yrs — is explicitly out of scope.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LwwRegister<T: Clone> {
    pub value: T,
    /// HLC u64 packed timestamp (D-447).
    pub written_at_hlc: u64,
}

impl<T: Clone> LwwRegister<T> {
    pub fn new(value: T, written_at_hlc: u64) -> Self {
        Self {
            value,
            written_at_hlc,
        }
    }

    /// Merge two replicas — newer HLC wins. Ties keep `self`.
    pub fn merge(&mut self, other: &LwwRegister<T>) {
        if other.written_at_hlc > self.written_at_hlc {
            self.value = other.value.clone();
            self.written_at_hlc = other.written_at_hlc;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrSet<T: Ord + Clone> {
    /// element → set of unique add-tags.
    adds: BTreeMap<T, BTreeSet<u64>>,
    /// removed-tags by element.
    removes: BTreeMap<T, BTreeSet<u64>>,
}

impl<T: Ord + Clone> OrSet<T> {
    pub fn add(&mut self, value: T, tag: u64) {
        self.adds.entry(value).or_default().insert(tag);
    }

    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.adds.get(value) {
            let snapshot: Vec<u64> = tags.iter().copied().collect();
            self.removes
                .entry(value.clone())
                .or_default()
                .extend(snapshot);
        }
    }

    #[must_use]
    pub fn contains(&self, value: &T) -> bool {
        let Some(adds) = self.adds.get(value) else {
            return false;
        };
        let removes = self.removes.get(value);
        adds.iter().any(|t| removes.is_none_or(|r| !r.contains(t)))
    }

    pub fn merge(&mut self, other: &Self) {
        for (k, tags) in &other.adds {
            self.adds
                .entry(k.clone())
                .or_default()
                .extend(tags.iter().copied());
        }
        for (k, tags) in &other.removes {
            self.removes
                .entry(k.clone())
                .or_default()
                .extend(tags.iter().copied());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lww_newer_wins() {
        let mut a = LwwRegister::new("a", 1);
        let b = LwwRegister::new("b", 2);
        a.merge(&b);
        assert_eq!(a.value, "b");
    }

    #[test]
    fn lww_older_loses() {
        let mut a = LwwRegister::new("a", 5);
        let b = LwwRegister::new("b", 1);
        a.merge(&b);
        assert_eq!(a.value, "a");
    }

    #[test]
    fn or_set_add_then_contains() {
        let mut s: OrSet<&'static str> = OrSet::default();
        s.add("x", 1);
        assert!(s.contains(&"x"));
    }

    #[test]
    fn or_set_remove_observed_add() {
        let mut s: OrSet<&'static str> = OrSet::default();
        s.add("x", 1);
        s.remove(&"x");
        assert!(!s.contains(&"x"));
    }

    #[test]
    fn or_set_concurrent_add_survives_old_remove() {
        let mut a: OrSet<&'static str> = OrSet::default();
        a.add("x", 1);
        let mut b = a.clone();
        b.remove(&"x");
        a.add("x", 2);
        a.merge(&b);
        assert!(a.contains(&"x"));
    }
}
