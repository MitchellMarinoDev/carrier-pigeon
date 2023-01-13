use hashbrown::HashMap;
use std::hash::Hash;

/// Uses 2 hashmaps allowing you to map with all the benefits of a hashmap
/// from key to value and value to key.
struct DoubleHashMap<K: Hash + Eq, V: Hash + Eq> {
    forward: HashMap<K, V>,
    backward: HashMap<V, K>,
}

impl<K: Hash + Eq, V: Hash + Eq> DoubleHashMap<K, V> {
    /// Gets a reference to the value associated with the given key.
    pub fn get(&self, key: impl AsRef<K>) -> Option<&V> {
        self.forward.get(key.as_ref())
    }

    /// Gets a mutable reference to the value associated with the given key.
    pub fn get_mut(&mut self, key: impl AsRef<K>) -> Option<&mut V> {
        self.forward.get_mut(key.as_ref())
    }

    /// Gets a reference to the key associated with the given value.
    pub fn get_backward(&self, key: impl AsRef<V>) -> Option<&K> {
        self.backward.get(key.as_ref())
    }

    /// Gets a mutable reference to the key associated with the given value.
    pub fn get_backward_mut(&mut self, key: impl AsRef<V>) -> Option<&mut K> {
        self.backward.get_mut(key.as_ref())
    }

    /// Inserts a key value pair into the map.
    pub fn insert(&mut self, key: K, value: V) {}
}
