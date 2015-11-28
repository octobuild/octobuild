use std::collections::VecDeque;
use std::cmp::min;
use std::io::Result;
pub use std::io::{Read, Write};

const BLOCK_SIZE: usize = 4000;

type Block = [u8; BLOCK_SIZE];

pub struct MemWrite {
	size: usize,
	blocks: VecDeque<Block>,
}

pub struct MemRead<'a> {
	offset: usize,
	writer: &'a MemWrite,
}

impl MemWrite {
	pub fn new() -> MemWrite {
		MemWrite {
			size: 0,
			blocks: VecDeque::new(),
		}
	}

	pub fn reader(&self) -> MemRead {
		MemRead {
			offset: 0,
			writer: self,
		}
	}
}

impl Write for MemWrite {
	fn write(&mut self, buf: &[u8]) -> Result<usize> {
		let mut offset = 0;
		while offset < buf.len() {
			if self.size % BLOCK_SIZE == 0 {
				self.blocks.push_back([0; BLOCK_SIZE]);
			};
			let mut block = self.blocks.back_mut().unwrap();
			let copy_offset = self.size % BLOCK_SIZE;
			let copy_size = min(buf.len() - offset, BLOCK_SIZE - copy_offset);
			for i in 0..copy_size {
				block[i] = buf[i];
			}
			offset += copy_size;
		}
		Ok(offset)
	}

	fn flush(&mut self) -> Result<()> {
		Ok(())
	}
}

impl<'a> Read for MemRead<'a> {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		Ok(0)
	}
}

#[cfg(test)]
mod test {
	extern crate rand;

	use super::{MemRead, MemWrite, BLOCK_SIZE};
	use std::io::{Read, Write};

	fn check_stream(write_size: usize, read_size: usize) {
		let mut size = 0;
		let mut expected: Vec<u8> = Vec::new();
		let mut writer = MemWrite::new();
		let mut rng = rand::thread_rng();
		while size < BLOCK_SIZE * 3 {
			let mut block = Vec::with_capacity(write_size);
			for i in 0..write_size {
				block[i] = rand::random::<u8>();
			}
			assert_eq!(writer.write(&block).unwrap(), write_size);
		}

		let mut actual = Vec::new();
		let mut reader = writer.reader();
		loop {
			let mut block = vec!(0; read_size);
			let size = reader.read(&mut block).unwrap();
			if size == 0 {
				break;
			}
			actual.write(&block[0..size]);
		}

		assert_eq!(expected, actual);
	}
}
