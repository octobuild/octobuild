use std::clone::Clone;
use std::sync::RwLock;

pub struct Lazy<T: Clone> {
    holder: RwLock<Option<T>>,
}

impl<T: Clone> Lazy<T> {
    pub fn new() -> Self {
        Lazy {
            holder: RwLock::new(None),
        }
    }

    pub fn get<F: FnOnce() -> T>(&self, factory: F) -> T {
        {
            let read_lock = self.holder.read().unwrap();
            match *read_lock {
                Some(ref v) => {
                    return v.clone();
                }
                None => {}
            }
        }
        let mut write_lock = self.holder.write().unwrap();
        if write_lock.is_none() {
            *write_lock = Some(factory());
        }
        write_lock.clone().unwrap()
    }
}
