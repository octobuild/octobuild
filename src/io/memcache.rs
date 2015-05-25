use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;

#[derive(Clone)]
pub struct MemCache<K, V> where K: Eq + Hash + Clone, V: Clone + Send {
	map: Arc<Mutex<HashMap<K, Arc<Mutex<Option<V>>>>>>,
}

impl<K, V> MemCache<K, V> where K: Eq + Hash + Clone, V: Clone + Send {
	pub fn new() -> Self {
		MemCache {
			map: Arc::new(Mutex::new(HashMap::new())),
		}
	}

	pub fn run_cached<F>(&self, key: K, worker: F) -> V where F: Fn(Option<V>) -> V {
		// Get/create lock for cache entry
		let entry_lock = match self.map.lock() {
			Ok(mut map) => {
				match map.entry(key) {
					Entry::Occupied(entry) => entry.get().clone(),
					Entry::Vacant(entry) => entry.insert(Arc::new(Mutex::new(None))).clone()
				}
			}
			Err(e) => panic! ("Mutex error: {}", e),
		};

		let result = match entry_lock.lock() {
			Ok(mut entry) => {
				let value = worker(entry.take());
				*entry = Some(value.clone());
				value
			}
			Err(e) => panic! ("Mutex error: {}", e),
		};

		result
	}
}
