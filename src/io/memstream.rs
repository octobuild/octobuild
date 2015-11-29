use std::collections::VecDeque;
use std::collections::vec_deque::Iter;
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
	size: usize,
	iter: Iter<'a, Block>,
	last: Option<&'a Block>,
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
			size: self.size,
			iter: self.blocks.iter(),
			last: None,
		}
	}
}

impl Write for MemWrite {
	fn write(&mut self, buf: &[u8]) -> Result<usize> {
		let mut src_offset = 0;
		while src_offset < buf.len() {
			let dst_offset = self.size % BLOCK_SIZE;
			if dst_offset == 0 {
				self.blocks.push_back([0; BLOCK_SIZE]);
			};
			let mut block = self.blocks.back_mut().unwrap();
			let copy_size = min(buf.len() - src_offset, BLOCK_SIZE - dst_offset);
			for i in 0..copy_size {
				block[dst_offset + i] = buf[src_offset + i];
			}
			self.size += copy_size;
			src_offset += copy_size;
		}
		Ok(src_offset)
	}

	fn flush(&mut self) -> Result<()> {
		Ok(())
	}
}

impl<'a> Read for MemRead<'a> {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		let mut dst_offset = 0;
		while (dst_offset < buf.len()) && (self.offset < self.size) {
			let src_offset = self.offset % BLOCK_SIZE;
			if src_offset == 0 {
				self.last = self.iter.next();
				assert!(self.last.is_some());
			}
			let copy_size = min(min(buf.len() - dst_offset, BLOCK_SIZE - src_offset), self.size - self.offset);
			let block = self.last.unwrap();
			for i in 0..copy_size {
				buf[dst_offset + i] = block[src_offset + i];
			}
		    // add code here
		    self.offset += copy_size;
		    dst_offset += copy_size;
		}
		Ok(dst_offset)
	}
}

#[cfg(test)]
mod test {
	extern crate rand;

	use super::{MemWrite, BLOCK_SIZE};
	use std::io::{Read, Write};

	fn check_stream(write_size: usize, read_size: usize) {
		let mut expected: Vec<u8> = Vec::new();
		let mut writer = MemWrite::new();
		while expected.len() < BLOCK_SIZE * 3 {
			let mut block = Vec::with_capacity(write_size);
			for _ in 0..write_size {
				block.push(rand::random::<u8>());
			}
			assert_eq!(writer.write(&block).unwrap(), write_size);
			assert_eq!(expected.write(&block).unwrap(), write_size);
		}

		let mut actual = Vec::new();
		let mut reader = writer.reader();
		let mut block = vec!(0; read_size);
		loop {
			let size = reader.read(&mut block).unwrap();
			if size == 0 {
				break;
			}
			actual.write(&block[0..size]).unwrap();
		}
		assert_eq!(expected.len(), actual.len());
		assert_eq!(expected, actual);
	}

	#[test]
	fn test_simple() {
		check_stream(BLOCK_SIZE, BLOCK_SIZE);
	}

	#[test]
	fn test_simple_half() {
		check_stream(BLOCK_SIZE / 2, BLOCK_SIZE / 2);
	}

	#[test]
	fn test_simple_one_and_half() {
		check_stream(BLOCK_SIZE * 3 / 2, BLOCK_SIZE * 3 / 2);
	}

	#[test]
	fn test_simple_1() {
		check_stream(1, 1);
	}

	#[test]
	fn test_simple_7() {
		check_stream(7, 7);
	}
}
