// This is a local util module. There will be some unneeded functionality.
#![allow(unused)]

use hashbrown::HashMap;
use std::hash::Hash;

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
pub enum DoubleHashMapError {
    /// The insert is invalid because either the key or value was already in the map.
    /// This would invalidate the
    InvalidInsert,
}

/// Uses 2 hashmaps allowing you to map with all the benefits of a hashmap
/// from key to value and value to key.
///
/// This map guarantees 1:1 value mapping.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct DoubleHashMap<K: Clone + Hash + Eq, V: Clone + Hash + Eq> {
    forward: HashMap<K, V>,
    backward: HashMap<V, K>,
}

impl<K: Clone + Hash + Eq, V: Clone + Hash + Eq> DoubleHashMap<K, V> {
    pub fn new() -> Self {
        DoubleHashMap {
            forward: HashMap::new(),
            backward: HashMap::new(),
        }
    }

    /// Gets a reference to the value associated with the given key.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.forward.get(key)
    }

    /// Gets a mutable reference to the value associated with the given key.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.forward.get_mut(key)
    }

    /// Gets a reference to the key associated with the given value.
    pub fn get_backward(&self, key: &V) -> Option<&K> {
        self.backward.get(key)
    }

    /// Gets a mutable reference to the key associated with the given value.
    pub fn get_backward_mut(&mut self, key: &V) -> Option<&mut K> {
        self.backward.get_mut(key)
    }

    /// Inserts a key value pair into the map.
    pub fn insert(&mut self, key: K, value: V) -> Result<(), DoubleHashMapError> {
        let old_key = self.backward.get(&value);
        let old_value = self.forward.get(&key);
        if (old_key.is_none() && old_value.is_none())
            || (old_key == Some(&key) && old_value == Some(&value))
        {
            self.forward.insert(key.clone(), value.clone());
            self.backward.insert(value, key);
            Ok(())
        } else {
            Err(DoubleHashMapError::InvalidInsert)
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let value = self.forward.remove(key)?;
        self.backward.remove(&value);
        Some(value)
    }

    pub fn remove_backward(&mut self, value: &V) -> Option<K> {
        let key = self.backward.remove(value)?;
        self.forward.remove(&key);
        Some(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.forward.contains_key(key)
    }

    pub fn contains_value(&self, value: &V) -> bool {
        self.backward.contains_key(value)
    }

    pub fn len(&self) -> usize {
        self.forward.len()
    }

    pub fn clear(&mut self) {
        self.forward.clear();
        self.backward.clear();
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.forward.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.backward.keys()
    }

    pub fn pairs(&self) -> impl Iterator<Item = (&K, &V)> {
        self.forward.iter()
    }
}
