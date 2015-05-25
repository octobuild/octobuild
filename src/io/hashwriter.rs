use std::io::{Write, Error};
use std::hash::Hasher;

pub struct HashWriter<T: Hasher> (T);

impl <T: Hasher> HashWriter<T> {
	pub fn new(hasher: T) -> HashWriter<T> {
		HashWriter(hasher)
	}

	pub fn unwrap(self) -> T {
            self.0
        }
}

impl <T: Hasher> Hasher for HashWriter<T> {
	fn finish(&self) -> u64 {
		self.0.finish()
	}

	fn write(&mut self, bytes: &[u8]) {
		self.0.write(bytes)
	}
}

impl <T: Hasher> Write for HashWriter<T> {
	fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
		self.0.write(buf);
		Ok(buf.len())
	}

	fn flush(&mut self) -> Result<(), Error> {
		Ok(())
	}
}
