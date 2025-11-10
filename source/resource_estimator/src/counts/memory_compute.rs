use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::hash::Hash;

#[cfg(test)]
mod tests;

pub enum CachingStrategy {
    LeastRecentlyUsed(LeastRecentlyUsedPriorityQueue<usize>),
    LeastFrequentlyUsed(LeastFrequentlyUsedPriorityQueue<usize>),
}

impl CachingStrategy {
    pub fn least_recently_used(capacity: usize) -> Self {
        CachingStrategy::LeastRecentlyUsed(LeastRecentlyUsedPriorityQueue::new(capacity))
    }

    pub fn least_frequently_used(capacity: usize) -> Self {
        CachingStrategy::LeastFrequentlyUsed(LeastFrequentlyUsedPriorityQueue::new(capacity))
    }
}

pub struct MemoryComputeInfo {
    /// LRU or LFU set with qubits currently in compute mode
    compute_qubits: CachingStrategy,

    /// Additional reads/writes not captured by the LRU or LFU set (e.g. when
    /// manually counted for caching functions)
    pub(crate) rfm_extra: usize,
    pub(crate) wtm_extra: usize,
}

impl MemoryComputeInfo {
    pub fn new(strategy: CachingStrategy) -> Self {
        Self {
            compute_qubits: strategy,
            rfm_extra: 0,
            wtm_extra: 0,
        }
    }

    pub fn assert_compute_qubits(&mut self, qubits: impl IntoIterator<Item = usize>) {
        match &mut self.compute_qubits {
            CachingStrategy::LeastRecentlyUsed(lru) => lru.insert_all(qubits),
            CachingStrategy::LeastFrequentlyUsed(lfu) => lfu.insert_all(qubits),
        }
    }

    pub fn compute_size(&self) -> usize {
        match &self.compute_qubits {
            CachingStrategy::LeastRecentlyUsed(lru) => lru.max_size(),
            CachingStrategy::LeastFrequentlyUsed(lfu) => lfu.max_size(),
        }
    }

    pub fn read_from_memory_count(&self) -> usize {
        match &self.compute_qubits {
            CachingStrategy::LeastRecentlyUsed(lru) => lru.inserted_new_count() + self.rfm_extra,
            CachingStrategy::LeastFrequentlyUsed(lfu) => lfu.inserted_new_count() + self.rfm_extra,
        }
    }

    pub fn write_to_memory_count(&self) -> usize {
        match &self.compute_qubits {
            CachingStrategy::LeastRecentlyUsed(lru) => lru.removed_count() + self.wtm_extra,
            CachingStrategy::LeastFrequentlyUsed(lfu) => lfu.removed_count() + self.wtm_extra,
        }
    }

    pub fn increase_read_from_memory_count(&mut self, count: usize) {
        self.rfm_extra += count;
    }

    pub fn increase_write_to_memory_count(&mut self, count: usize) {
        self.wtm_extra += count;
    }
}

/// LRU priority queue / set. Maintains up to `capacity` distinct keys; eviction
/// removes the least recently used key.
#[derive(Debug)]
pub struct LeastRecentlyUsedPriorityQueue<K> {
    // Set of keys for O(1) membership testing.
    map: FxHashSet<K>,
    // Deque of keys in recency order (most recent at front).
    nodes: VecDeque<K>,
    // Maximum number of distinct keys to hold.
    capacity: usize,
    // Number of times a key was newly inserted (was not present beforehand).
    inserted_new: usize,
    // Number of times a key was removed due to eviction or explicit pop.
    removed: usize,
    // Maximum size reached at any point in time.
    max_size: usize,
}

impl<K: Eq + Hash + Clone> LeastRecentlyUsedPriorityQueue<K> {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: FxHashSet::with_capacity_and_hasher(capacity, Default::default()),
            nodes: VecDeque::with_capacity(capacity),
            capacity,
            inserted_new: 0,
            removed: 0,
            max_size: 0,
        }
    }

    pub fn inserted_new_count(&self) -> usize {
        self.inserted_new
    }

    pub fn removed_count(&self) -> usize {
        self.removed
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    fn contains(&self, key: &K) -> bool {
        self.map.contains(key)
    }

    /// Insert multiple keys ensuring they are all present afterwards. If more
    /// unique new keys are provided than capacity, only the most recently
    /// processed up to `capacity` will remain.
    pub fn insert_all<I: IntoIterator<Item = K>>(&mut self, keys: I) {
        if self.capacity == 0 {
            return;
        }
        // Collect unique keys from input preserving order of first occurrence.
        let mut seen_input = FxHashSet::default();
        let mut ordered: Vec<K> = Vec::new();
        for k in keys {
            if seen_input.insert(k.clone()) {
                ordered.push(k);
            }
        }
        debug_assert!(
            ordered.len() <= self.capacity,
            "More keys than capacity in LruPQ::insert_all"
        );

        // Process each key in order; we evict as we go and since new elements
        // are moved front they will be retained if we exceed capacity.
        for k in ordered {
            if self.contains(&k) {
                // Just update recency by moving element to front of deque
                if let Some(value) = self
                    .nodes
                    .iter()
                    .position(|n| n == &k)
                    .and_then(|i| self.nodes.remove(i))
                {
                    self.nodes.push_front(value);
                }
            } else {
                // Evict if at capacity
                if self.map.len() == self.capacity {
                    if let Some(key) = self.nodes.pop_back() {
                        self.map.remove(&key);
                        self.removed += 1;
                    }
                }
                self.map.insert(k.clone());
                self.nodes.push_front(k);
                self.inserted_new += 1;
            }
        }

        if self.map.len() > self.max_size {
            self.max_size = self.map.len();
        }
    }
}

/// LFU priority queue / set. Maintains up to `capacity` distinct keys; eviction
/// removes the key with lowest frequency (ties broken by oldest insertion among
/// that frequency bucket).
pub struct LeastFrequentlyUsedPriorityQueue<K> {
    // Map of keys to their frequencies.
    map: FxHashMap<K, u64>,
    // Same-frequency buckets with ordered keys (oldest at front).
    freq_buckets: FxHashMap<u64, VecDeque<K>>,
    // Minimum frequency of any key in the structure (for eviction).
    min_freq: u64,
    // Maximum number of distinct keys to hold.
    capacity: usize,
    // Number of times a key was newly inserted (was not present beforehand).
    inserted_new: usize,
    // Number of times a key was removed due to eviction or explicit pop.
    removed: usize,
    // Maximum size reached at any point in time.
    max_size: usize,
}

impl<K: Eq + Hash + Clone> LeastFrequentlyUsedPriorityQueue<K> {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            freq_buckets: FxHashMap::default(),
            min_freq: 0,
            capacity,
            inserted_new: 0,
            removed: 0,
            max_size: 0,
        }
    }

    pub fn inserted_new_count(&self) -> usize {
        self.inserted_new
    }

    pub fn removed_count(&self) -> usize {
        self.removed
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Insert multiple keys ensuring they are all present afterwards. If unique
    /// keys exceed capacity, only a subset up to capacity will remain.
    pub fn insert_all<I: IntoIterator<Item = K>>(&mut self, keys: I) {
        if self.capacity == 0 {
            return;
        }
        let mut seen = FxHashSet::default();
        let mut ordered: Vec<K> = Vec::new();
        for k in keys {
            if seen.insert(k.clone()) {
                ordered.push(k);
            }
        }
        debug_assert!(
            ordered.len() <= self.capacity,
            "More keys than capacity in LfuPQ::insert_all"
        );

        // Evict as needed to make space for new keys.  We need to evict before
        // adding the new elements, since frequency counters are low for new
        // elements and we risk to evict them before processing the whole input.
        let new_missing = ordered
            .iter()
            .filter(|k| !self.map.contains_key(*k))
            .count();
        if new_missing > 0 {
            // Pre-evict keys not in incoming set according to LFU policy until
            // space
            let incoming_set: FxHashSet<K> = ordered.iter().cloned().collect();
            let mut needed = self.map.len() + new_missing;
            while needed > self.capacity {
                // choose victim: lowest freq, oldest within bucket, not in
                // incoming_set
                let mut freq = self.min_freq;
                let mut victim: Option<K> = None;
                while victim.is_none() {
                    if let Some(bucket) = self.freq_buckets.get(&freq) {
                        for key in bucket {
                            if !incoming_set.contains(key) {
                                victim = Some(key.clone());
                                break;
                            }
                        }
                    }
                    if victim.is_none() {
                        freq += 1;
                    }
                }
                if let Some(v) = victim {
                    self.remove_key_internal(&v);
                    needed -= 1;
                } else {
                    break;
                }
            }
        }

        // Now apply each key: bump freq if existing else insert new
        for k in ordered {
            if let Some(freq) = self.map.get_mut(&k) {
                *freq += 1;
                let old = *freq - 1;
                let new = *freq;
                self.bump_bucket(k.clone(), old, new);
            } else {
                self.map.insert(k.clone(), 1);
                self.freq_buckets.entry(1).or_default().push_back(k);
                self.min_freq = 1;
                self.inserted_new += 1;
            }
        }

        if self.map.len() > self.max_size {
            self.max_size = self.map.len();
        }
    }

    fn bump_bucket(&mut self, key: K, old_freq: u64, new_freq: u64) {
        if let Some(bucket) = self.freq_buckets.get_mut(&old_freq) {
            if let Some(pos) = bucket.iter().position(|k| k == &key) {
                bucket.remove(pos);
            }
            if bucket.is_empty() {
                self.freq_buckets.remove(&old_freq);
                if self.min_freq == old_freq {
                    self.min_freq = new_freq;
                }
            }
        }
        self.freq_buckets
            .entry(new_freq)
            .or_default()
            .push_back(key);
    }

    /// Remove a given key without returning it (used by bulk insertion eviction
    /// logic).
    fn remove_key_internal(&mut self, key: &K) {
        if let Some(freq) = self.map.remove(key) {
            // Remove from its frequency bucket
            if let Some(bucket) = self.freq_buckets.get_mut(&freq) {
                if let Some(pos) = bucket.iter().position(|k| k == key) {
                    bucket.remove(pos);
                }
                if bucket.is_empty() {
                    self.freq_buckets.remove(&freq);
                }
            }
            // Recompute min_freq if needed (lazy: set to smallest existing key
            // or 0)
            if self.min_freq == freq {
                self.min_freq = self.freq_buckets.keys().min().copied().unwrap_or(0);
            }
            self.removed += 1;
        }
    }
}
