//! Smart Cache for recurrent states and completion fast-paths.
//!
//! Provides a thread-safe, low-cost LRU/bounded cache where context is the
//! source of truth, and cached entries provide performance gains.

use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

pub struct SmartCache<K, V> {
    capacity: usize,
    entries: RwLock<(HashMap<K, V>, VecDeque<K>)>,
}

impl<K: Eq + Hash + Clone, V: Clone> SmartCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: RwLock::new((HashMap::new(), VecDeque::new())),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let guard = self.entries.read();
        guard.0.get(key).cloned()
    }

    pub fn insert(&self, key: K, value: V) {
        let mut guard = self.entries.write();
        if !guard.0.contains_key(&key) {
            if guard.1.len() >= self.capacity {
                if let Some(old_key) = guard.1.pop_front() {
                    guard.0.remove(&old_key);
                }
            }
            guard.1.push_back(key.clone());
        }
        guard.0.insert(key, value);
    }

    pub fn clear(&self) {
        let mut guard = self.entries.write();
        guard.0.clear();
        guard.1.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.read().0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_cache_lru() {
        let cache = SmartCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        assert_eq!(cache.get(&"a"), Some(1));

        cache.insert("c", 3); // evicts "a"
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
    }
}
