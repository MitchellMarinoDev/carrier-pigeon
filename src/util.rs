use hashbrown::HashMap;
use std::hash::Hash;

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
enum DoubleHashMapError {
    /// The insert is invalid because either the key or value was already in the map.
    /// This would invalidate the
    InvalidInsert,
}

/// Uses 2 hashmaps allowing you to map with all the benefits of a hashmap
/// from key to value and value to key.
///
/// This map guarantees 1:1 value mapping.
#[derive(Clone, Eq, PartialEq, Debug)]
struct DoubleHashMap<K: Clone + Hash + Eq, V: Clone + Hash + Eq> {
    forward: HashMap<K, V>,
    backward: HashMap<V, K>,
}

impl<K: Clone + Hash + Eq, V: Clone + Hash + Eq> DoubleHashMap<K, V> {
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
    pub fn insert(&mut self, key: K, value: V) -> Result<(), DoubleHashMapError>{
        let old_key = self.backward.get(&value);
        let old_value = self.forward.get(&key);
        if (old_key == None && old_value == None) || (old_key == Some(&key) && old_value == Some(&value)) {
            self.forward.insert(key.clone(), value.clone());
            self.backward.insert(value, key);
            Ok(())
        } else {
            Err(DoubleHashMapError::InvalidInsert)
        }
    }

    pub fn remove(&mut self, key: impl AsRef<K>) -> Option<V> {
        let value = self.forward.remove(key.as_ref())?;
        self.backward.remove(&value);
        Some(value)
    }

    pub fn remove_backward(&mut self, value: impl AsRef<V>) -> Option<K> {
        let key = self.backward.remove(value.as_ref())?;
        self.forward.remove(&key);
        Some(key)
    }

    pub fn len(&self) -> usize {
        self.forward.len()
    }

    pub fn clear(&mut self) {
        self.forward.clear();
        self.backward.clear();
    }
}
