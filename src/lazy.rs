use std::clone::Clone;
use std::sync::RwLock;

pub struct Lazy<T: Clone> {
    holder: RwLock<Option<T>>,
}

impl<T: Clone> Default for Lazy<T> {
    fn default() -> Self {
        Lazy {
            holder: RwLock::new(None),
        }
    }
}

impl<T: Clone> Lazy<T> {
    pub fn get<F: FnOnce() -> T>(&self, factory: F) -> T {
        {
            let read_lock = self.holder.read().unwrap();
            if let Some(ref v) = *read_lock {
                return v.clone();
            }
        }
        let mut write_lock = self.holder.write().unwrap();
        if write_lock.is_none() {
            *write_lock = Some(factory());
        }
        write_lock.clone().unwrap()
    }
}
