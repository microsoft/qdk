use super::{LeastFrequentlyUsedPriorityQueue, LeastRecentlyUsedPriorityQueue};

// ---------------- LRU tests -----------------
#[test]
fn lru_insert_all_all_existing() {
    let mut lru = LeastRecentlyUsedPriorityQueue::new(4);
    lru.insert_all(["a", "b", "c"]);
    assert_eq!(lru.inserted_new_count(), 3);
    assert_eq!(lru.removed_count(), 0);
    lru.insert_all(["b", "c"]); // no new keys
    assert_eq!(lru.inserted_new_count(), 3);
    assert_eq!(lru.removed_count(), 0);
    assert_eq!(lru.max_size(), 3);
}

#[test]
fn lru_insert_all_some_new_no_eviction() {
    let mut lru = LeastRecentlyUsedPriorityQueue::new(5);
    lru.insert_all(["a", "b"]);
    lru.insert_all(["b", "c", "d"]); // adds c,d
    assert_eq!(lru.inserted_new_count(), 4); // a,b,c,d
    assert_eq!(lru.removed_count(), 0);
    assert_eq!(lru.max_size(), 4);
}

#[test]
fn lru_insert_all_with_eviction() {
    let mut lru = LeastRecentlyUsedPriorityQueue::new(3);
    lru.insert_all(["a", "b"]);
    lru.insert_all(["b", "c", "d"]); // evicts a, inserts c,d
    assert_eq!(lru.inserted_new_count(), 4); // a,b,c,d
    assert_eq!(lru.removed_count(), 1); // a
    // Set currently holds b,c,d
    assert_eq!(lru.max_size(), 3);
}

#[test]
fn lru_insert_all_complex_eviction() {
    let mut lru = LeastRecentlyUsedPriorityQueue::new(4);
    lru.insert_all(["a", "b", "c"]);
    // Touch a to make it most recent, order (a, c, b, ... LRU is b)
    lru.insert_all(["a"]);
    lru.insert_all(["a", "d", "e", "f"]); // need to keep a,d,e,f; evict b & c
    assert_eq!(lru.inserted_new_count(), 6); // a,b,c,d,e,f
    assert_eq!(lru.removed_count(), 2); // b,c
    assert_eq!(lru.max_size(), 4);
}

#[test]
fn lru_insert_all_duplicates_in_input() {
    let mut lru = LeastRecentlyUsedPriorityQueue::new(5);
    lru.insert_all(["a", "b", "b", "c", "a", "d"]); // duplicates should not inflate count
    assert_eq!(lru.inserted_new_count(), 4); // a,b,c,d
    assert_eq!(lru.removed_count(), 0);
    assert_eq!(lru.max_size(), 4);
}

#[test]
fn lru_insert_all_recency_update_only() {
    let mut lru = LeastRecentlyUsedPriorityQueue::new(3);
    lru.insert_all(["x", "y", "z"]);
    assert_eq!(lru.inserted_new_count(), 3);
    lru.insert_all(["z", "y"]); // no new insertions
    assert_eq!(lru.inserted_new_count(), 3);
    assert_eq!(lru.removed_count(), 0);
    assert_eq!(lru.max_size(), 3);
    // Adding one new triggers eviction of least recently used (x)
    lru.insert_all(["y", "w"]);
    assert_eq!(lru.inserted_new_count(), 4); // w added
    assert_eq!(lru.removed_count(), 1); // x removed
}

// ---------------- LFU tests -----------------
#[test]
fn lfu_basic_frequency_eviction() {
    let mut lfu = LeastFrequentlyUsedPriorityQueue::new(3);
    lfu.insert_all(["a"]);
    lfu.insert_all(["b"]);
    lfu.insert_all(["c"]);
    // Bump b twice
    lfu.insert_all(["b"]);
    lfu.insert_all(["b"]);
    // Insert d -> should evict a (oldest among lowest freq =1 keys a & c)
    lfu.insert_all(["d"]);
    assert!(!lfu.map.contains_key(&"a"));
    assert!(lfu.map.contains_key(&"b"));
    assert!(lfu.map.contains_key(&"c"));
    assert!(lfu.map.contains_key(&"d"));
}

#[test]
fn lfu_update_existing_value() {
    let mut lfu = LeastFrequentlyUsedPriorityQueue::new(1);
    lfu.insert_all(["a"]);
    lfu.insert_all(["a"]); // bump freq
    assert!(lfu.map.contains_key(&"a"));
    lfu.insert_all(["b"]); // eviction of a
    assert!(!lfu.map.contains_key(&"a"));
    assert!(lfu.map.contains_key(&"b"));
}

#[test]
fn lfu_zero_capacity() {
    let mut lfu: LeastFrequentlyUsedPriorityQueue<&str> = LeastFrequentlyUsedPriorityQueue::new(0);
    lfu.insert_all(["a"]); // ignored
    assert_eq!(lfu.inserted_new_count(), 0);
    assert_eq!(lfu.removed_count(), 0);
    assert_eq!(lfu.max_size(), 0);
}

#[test]
fn lfu_eviction_prefers_lowest_freq_oldest() {
    let mut lfu = LeastFrequentlyUsedPriorityQueue::new(3);
    lfu.insert_all(["x"]);
    lfu.insert_all(["y"]);
    lfu.insert_all(["z"]);
    // bump x twice to freq=3
    lfu.insert_all(["x"]);
    lfu.insert_all(["x"]);
    // Insert w -> evict y (oldest among y,z with lowest freq=1)
    lfu.insert_all(["w"]);
    assert!(!lfu.map.contains_key(&"y"));
    assert!(lfu.map.contains_key(&"x"));
    assert!(lfu.map.contains_key(&"z"));
    assert!(lfu.map.contains_key(&"w"));
}

#[test]
fn lfu_insert_all() {
    let mut lfu = LeastFrequentlyUsedPriorityQueue::new(3);
    lfu.insert_all(["a"]);
    lfu.insert_all(["b"]);
    lfu.insert_all(["b"]); // bump b
    lfu.insert_all(["c"]);
    // frequencies: a:1, b:2, c:1 (a oldest among freq1)
    lfu.insert_all(["d", "e"]); // need space for 2 new: evict a then c
    assert!(lfu.map.contains_key(&"d"));
    assert!(lfu.map.contains_key(&"e"));
    // b should remain (highest freq)
    assert!(lfu.map.contains_key(&"b"));
    assert!(!lfu.map.contains_key(&"a"));
    assert!(!lfu.map.contains_key(&"c"));
    assert_eq!(lfu.map.len(), 3);
}

#[test]
fn lfu_counters() {
    let mut lfu = LeastFrequentlyUsedPriorityQueue::new(3);
    lfu.insert_all(["a"]);
    lfu.insert_all(["b"]);
    lfu.insert_all(["c"]);
    assert_eq!(lfu.inserted_new_count(), 3);
    assert_eq!(lfu.max_size(), 3);
    lfu.insert_all(["b"]); // bump b
    lfu.insert_all(["d", "e"]); // inserts d,e removes a,c
    assert_eq!(lfu.inserted_new_count(), 5);
    assert_eq!(lfu.removed_count(), 2);
    // Another eviction
    lfu.insert_all(["f"]); // inserts f removes one of (d,e)
    assert_eq!(lfu.inserted_new_count(), 6);
    assert_eq!(lfu.map.len(), 3);
    assert!(lfu.removed_count() >= 3);
    assert_eq!(lfu.max_size(), 3);
}

#[test]
fn lfu_max_size_tracking() {
    let mut lfu = LeastFrequentlyUsedPriorityQueue::new(4);
    assert_eq!(lfu.max_size(), 0);
    lfu.insert_all(["a"]);
    lfu.insert_all(["b"]);
    lfu.insert_all(["c"]);
    assert_eq!(lfu.max_size(), 3);
    lfu.insert_all(["c"]); // duplicate bump
    assert_eq!(lfu.max_size(), 3);
    lfu.insert_all(["d"]);
    assert_eq!(lfu.max_size(), 4);
    lfu.insert_all(["e", "f"]); // still capacity 4
    assert_eq!(lfu.max_size(), 4);
}
