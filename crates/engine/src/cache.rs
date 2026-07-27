//! Smart Cache for recurrent states and completion fast-paths.
//!
//! Provides a thread-safe, bounded LRU cache where context is the source of
//! truth and cached entries provide performance gains.
//!
//! ## What was wrong before
//!
//! The original implementation had three defects that made it a slow leak
//! rather than a cache:
//!
//! 1. **It was FIFO, not LRU.** `get` never touched the recency queue, so the
//!    eviction order was insertion order. A hot key inserted early was evicted
//!    while cold keys inserted later survived — the opposite of the intent.
//! 2. **Re-inserting an existing key never refreshed recency**, and pushed no
//!    queue entry, so `entries.len()` and `queue.len()` could disagree.
//! 3. **Only entry *count* was bounded.** For the intended payload —
//!    serialized RWKV recurrent states, megabytes each — a 128-entry cache is
//!    a multi-gigabyte cache. There was no way to bound actual memory.
//!
//! This version fixes the recency tracking and adds an optional byte budget
//! plus a TTL, so cached state cannot outlive its usefulness or eat the heap.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// Anything that can report its own memory footprint, so the cache can bound
/// bytes rather than just entry count.
pub trait Weighed {
    /// Approximate heap size of this value in bytes.
    fn weight(&self) -> usize;
}

impl Weighed for Vec<u8> {
    fn weight(&self) -> usize {
        self.len()
    }
}

impl Weighed for String {
    fn weight(&self) -> usize {
        self.len()
    }
}

impl<T: Weighed> Weighed for Option<T> {
    fn weight(&self) -> usize {
        self.as_ref().map_or(0, Weighed::weight)
    }
}

/// Blanket small-value weight for `Copy` scalars used in tests and counters.
macro_rules! impl_weighed_scalar {
    ($($t:ty),*) => {
        $(impl Weighed for $t {
            fn weight(&self) -> usize { std::mem::size_of::<$t>() }
        })*
    };
}
impl_weighed_scalar!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, bool, f32, f64);

struct Entry<V> {
    value: V,
    /// Monotonic counter; higher means more recently used.
    stamp: u64,
    inserted: Instant,
    weight: usize,
}

/// Bounded, thread-safe LRU cache with optional byte and time budgets.
pub struct SmartCache<K, V> {
    capacity: usize,
    /// Optional byte budget. `None` = count-bounded only.
    max_bytes: Option<usize>,
    /// Optional entry lifetime. `None` = entries never expire.
    ttl: Option<Duration>,
    inner: Mutex<Inner<K, V>>,
}

struct Inner<K, V> {
    entries: HashMap<K, Entry<V>>,
    clock: u64,
    bytes: usize,
}

impl<K: Eq + Hash + Clone, V: Clone + Weighed> SmartCache<K, V> {
    /// A cache bounded by entry count.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            max_bytes: None,
            ttl: None,
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                clock: 0,
                bytes: 0,
            }),
        }
    }

    /// Also bound total cached bytes. Essential when caching recurrent states,
    /// where a single entry can be megabytes.
    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = Some(max_bytes.max(1));
        self
    }

    /// Expire entries older than `ttl`.
    ///
    /// Without this, a long-lived daemon keeps state for sessions that ended
    /// hours ago: memory that is live, unreachable by any user, and never
    /// evicted because it is never the least-recently-used entry.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Look up a key, marking it as recently used.
    pub fn get(&self, key: &K) -> Option<V> {
        let mut guard = self.inner.lock();
        // Expire first so a stale entry is never returned.
        if let Some(ttl) = self.ttl {
            if guard
                .entries
                .get(key)
                .is_some_and(|e| e.inserted.elapsed() > ttl)
            {
                if let Some(e) = guard.entries.remove(key) {
                    guard.bytes = guard.bytes.saturating_sub(e.weight);
                }
                return None;
            }
        }
        guard.clock += 1;
        let stamp = guard.clock;
        let entry = guard.entries.get_mut(key)?;
        // Promote on access — this is what makes it an LRU rather than a FIFO.
        entry.stamp = stamp;
        Some(entry.value.clone())
    }

    /// Insert or replace a value, evicting as needed to stay within budget.
    pub fn insert(&self, key: K, value: V) {
        let weight = value.weight();
        let mut guard = self.inner.lock();
        guard.clock += 1;
        let stamp = guard.clock;

        if let Some(old) = guard.entries.insert(
            key,
            Entry {
                value,
                stamp,
                inserted: Instant::now(),
                weight,
            },
        ) {
            guard.bytes = guard.bytes.saturating_sub(old.weight);
        }
        guard.bytes += weight;

        self.evict(&mut guard);
    }

    /// Remove a key, returning whether it was present.
    pub fn remove(&self, key: &K) -> bool {
        let mut guard = self.inner.lock();
        match guard.entries.remove(key) {
            Some(e) => {
                guard.bytes = guard.bytes.saturating_sub(e.weight);
                true
            }
            None => false,
        }
    }

    /// Drop every entry.
    pub fn clear(&self) {
        let mut guard = self.inner.lock();
        guard.entries.clear();
        guard.bytes = 0;
    }

    /// Drop expired entries. Safe to call periodically from a daemon loop.
    pub fn purge_expired(&self) -> usize {
        let Some(ttl) = self.ttl else { return 0 };
        let mut guard = self.inner.lock();
        let before = guard.entries.len();
        let mut freed = 0usize;
        guard.entries.retain(|_, e| {
            let keep = e.inserted.elapsed() <= ttl;
            if !keep {
                freed += e.weight;
            }
            keep
        });
        guard.bytes = guard.bytes.saturating_sub(freed);
        before - guard.entries.len()
    }

    /// Number of live entries.
    pub fn len(&self) -> usize {
        self.inner.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total cached bytes, as reported by [`Weighed`].
    pub fn bytes(&self) -> usize {
        self.inner.lock().bytes
    }

    /// Configured entry-count capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    // ── internals ────────────────────────────────────────────────────────

    /// Evict expired entries, then the least-recently-used, until both the
    /// count and byte budgets are satisfied.
    fn evict(&self, guard: &mut Inner<K, V>) {
        if let Some(ttl) = self.ttl {
            let mut freed = 0usize;
            guard.entries.retain(|_, e| {
                let keep = e.inserted.elapsed() <= ttl;
                if !keep {
                    freed += e.weight;
                }
                keep
            });
            guard.bytes = guard.bytes.saturating_sub(freed);
        }

        loop {
            let over_count = guard.entries.len() > self.capacity;
            let over_bytes = self
                .max_bytes
                .is_some_and(|max| guard.bytes > max && guard.entries.len() > 1);
            if !(over_count || over_bytes) {
                return;
            }
            // Find the least-recently-used key.
            let Some(victim) = guard
                .entries
                .iter()
                .min_by_key(|(_, e)| e.stamp)
                .map(|(k, _)| k.clone())
            else {
                return;
            };
            if let Some(e) = guard.entries.remove(&victim) {
                guard.bytes = guard.bytes.saturating_sub(e.weight);
            } else {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_cache_lru() {
        let cache = SmartCache::new(2);
        cache.insert("a", 1u32);
        cache.insert("b", 2);
        assert_eq!(cache.get(&"a"), Some(1));

        cache.insert("c", 3);
        // "a" was just accessed, so "b" is the least-recently-used victim.
        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), None);
        assert_eq!(cache.get(&"c"), Some(3));
    }

    #[test]
    fn get_promotes_so_hot_keys_survive() {
        // Regression: the old cache was FIFO, so a repeatedly-read key was
        // evicted while never-read newer keys stayed.
        let cache = SmartCache::new(3);
        cache.insert("hot", 0u32);
        cache.insert("b", 1);
        cache.insert("c", 2);

        for _ in 0..10 {
            assert_eq!(cache.get(&"hot"), Some(0));
            cache.insert("churn", 9);
        }
        assert_eq!(
            cache.get(&"hot"),
            Some(0),
            "hot key should never be evicted"
        );
    }

    #[test]
    fn reinsert_updates_value_and_recency_without_growing() {
        let cache = SmartCache::new(2);
        cache.insert("a", 1u32);
        cache.insert("a", 2);
        assert_eq!(cache.len(), 1, "re-insert must not duplicate the key");
        assert_eq!(cache.get(&"a"), Some(2));

        // "a" was just accessed via get(), so it's the MRU.
        // Inserting "b" gives it a newer stamp than "a".
        cache.insert("b", 3);
        cache.insert("b", 3); // re-insert "b" to refresh its stamp
        cache.insert("c", 4); // evicts the LRU "a" (lowest stamp)
        assert_eq!(cache.get(&"a"), None, "a was LRU and should be evicted");
        assert_eq!(
            cache.get(&"b"),
            Some(3),
            "b was re-inserted and should survive"
        );
        assert_eq!(cache.get(&"c"), Some(4), "c is newest");
    }

    #[test]
    fn entry_count_never_exceeds_capacity() {
        let cache = SmartCache::new(8);
        for i in 0..1_000u32 {
            cache.insert(i, i);
        }
        assert!(cache.len() <= 8, "len = {}", cache.len());
    }

    #[test]
    fn byte_budget_is_enforced() {
        // 10 entries would fit by count, but not by bytes.
        let cache: SmartCache<u32, Vec<u8>> = SmartCache::new(100).with_max_bytes(1_000);
        for i in 0..50u32 {
            cache.insert(i, vec![0u8; 200]);
        }
        assert!(
            cache.bytes() <= 1_000,
            "byte budget exceeded: {}",
            cache.bytes()
        );
        assert!(cache.len() <= 5, "len = {}", cache.len());
    }

    #[test]
    fn bytes_are_accounted_on_replace_and_remove() {
        let cache: SmartCache<&str, Vec<u8>> = SmartCache::new(4);
        cache.insert("k", vec![0u8; 100]);
        assert_eq!(cache.bytes(), 100);
        cache.insert("k", vec![0u8; 10]);
        assert_eq!(cache.bytes(), 10, "replace must not double-count");
        assert!(cache.remove(&"k"));
        assert_eq!(cache.bytes(), 0);
        assert!(!cache.remove(&"k"));
    }

    #[test]
    fn a_single_oversized_entry_is_still_retrievable() {
        // Evicting down to zero would make the cache useless for large states.
        let cache: SmartCache<&str, Vec<u8>> = SmartCache::new(4).with_max_bytes(10);
        cache.insert("big", vec![0u8; 5_000]);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&"big").is_some());
    }

    #[test]
    fn ttl_expires_entries() {
        let cache: SmartCache<&str, u32> = SmartCache::new(10).with_ttl(Duration::from_millis(20));
        cache.insert("a", 1);
        assert_eq!(cache.get(&"a"), Some(1));
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(cache.get(&"a"), None, "entry should have expired");
    }

    #[test]
    fn purge_expired_reclaims_bytes() {
        let cache: SmartCache<u32, Vec<u8>> =
            SmartCache::new(100).with_ttl(Duration::from_millis(20));
        for i in 0..10u32 {
            cache.insert(i, vec![0u8; 100]);
        }
        assert_eq!(cache.bytes(), 1_000);
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(cache.purge_expired(), 10);
        assert_eq!(cache.bytes(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn clear_resets_everything() {
        let cache: SmartCache<u32, Vec<u8>> = SmartCache::new(10);
        for i in 0..5u32 {
            cache.insert(i, vec![0u8; 10]);
        }
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn zero_capacity_is_clamped_to_one() {
        let cache = SmartCache::new(0);
        cache.insert("a", 1u32);
        assert_eq!(cache.capacity(), 1);
        assert_eq!(cache.get(&"a"), Some(1));
    }

    #[test]
    fn concurrent_access_stays_bounded() {
        use std::sync::Arc;
        let cache: Arc<SmartCache<u32, Vec<u8>>> =
            Arc::new(SmartCache::new(16).with_max_bytes(4_096));
        let mut handles = Vec::new();
        for t in 0..8u32 {
            let c = Arc::clone(&cache);
            handles.push(std::thread::spawn(move || {
                for i in 0..500u32 {
                    c.insert(t * 1000 + i, vec![0u8; 64]);
                    let _ = c.get(&(t * 1000 + i / 2));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(cache.len() <= 16, "len = {}", cache.len());
        assert!(cache.bytes() <= 4_096, "bytes = {}", cache.bytes());
    }
}
