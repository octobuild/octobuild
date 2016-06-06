use std::collections::VecDeque;
use std::collections::vec_deque;
use std::cmp::min;
use std::mem;
use std::ptr;
use std::hash::Hasher;
use std::io::Result;
pub use std::io::{Read, Write};

const BLOCK_SIZE: usize = 0x10000 - 0x100;

type Block = [u8; BLOCK_SIZE];

pub struct MemStream {
	size: usize,
	blocks: VecDeque<Block>,
}

pub struct Iter<'a> {
	size: usize,
	iter: vec_deque::Iter<'a, Block>,
}

pub struct MemReader<'a> {
	offset: usize,
	iter: Iter<'a>,
	last: Option<&'a [u8]>,
}

impl MemStream {
	pub fn new() -> Self {
		MemStream {
			size: 0,
			blocks: VecDeque::new(),
		}
	}

	pub fn size(&self) -> usize {
		return self.size;
	}

	pub fn reader(&self) -> MemReader {
		let mut iter = self.iter();
		let last = iter.next();
		MemReader {
			offset: 0,
			iter: iter,
			last: last,
		}
	}

	pub fn iter(&self) -> Iter {
		Iter {
			size: self.size,
			iter: self.blocks.iter(),
		}
	}

	pub fn copy<W: Write>(&self, writer: &mut W) -> Result<usize> {
		for block in self.iter() {
			try!(writer.write(block));
		}
		Ok(self.size)
	}

	pub fn hash<H: Hasher>(&self, hasher: &mut H) {
		hasher.write_usize(self.size);
		for block in self.iter() {
			hasher.write(block);
		}
	}
}

fn memcpy(src: &[u8], dst: &mut [u8]) {
	assert!(src.len() == dst.len());
	unsafe {
		ptr::copy_nonoverlapping(&src[0], &mut dst[0], src.len());
	}
}

impl Write for MemStream {
	fn write(&mut self, buf: &[u8]) -> Result<usize> {
		let mut src_offset = 0;
		while src_offset < buf.len() {
			let dst_offset = self.size % BLOCK_SIZE;
			if dst_offset == 0 {
				self.blocks.push_back(unsafe {mem::uninitialized()});
			};
			let mut block = self.blocks.back_mut().unwrap();
			let copy_size = min(buf.len() - src_offset, BLOCK_SIZE - dst_offset);
			memcpy(&buf[src_offset..src_offset + copy_size], &mut block[dst_offset..dst_offset + copy_size]);
			self.size += copy_size;
			src_offset += copy_size;
		}
		Ok(src_offset)
	}

	fn flush(&mut self) -> Result<()> {
		Ok(())
	}
}

impl<'a> Iterator for Iter<'a> {
	type Item = &'a [u8];

	fn next(&mut self) -> Option<Self::Item> {
		if self.size > 0 {
			let block = self.iter.next().unwrap();
			let size = min(self.size, block.len());
			self.size -= size;
			Some(&block[0..size])
		} else {
			None
		}
	}
}

impl<'a> Read for MemReader<'a> {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		let mut dst_offset = 0;
		while dst_offset < buf.len() {
			match self.last {
			    Some(block) => {
			    	if self.offset == block.len() {
			    		self.last = self.iter.next();
			    		self.offset = 0;
			    		continue;
			    	}
					let copy_size = min(buf.len() - dst_offset, block.len() - self.offset);
					memcpy(&block[self.offset..self.offset + copy_size], &mut buf[dst_offset..dst_offset + copy_size]);
					// add code here
					self.offset += copy_size;
					dst_offset += copy_size;
			    }
			    None => {
			    	break;
			    }
			}
		}
		Ok(dst_offset)
	}
}

#[cfg(test)]
mod test {
	extern crate rand;

	use super::{MemStream, BLOCK_SIZE};
	use std::io::{Read, Write};

	fn check_stream(write_size: usize, read_size: usize) {
		let mut expected: Vec<u8> = Vec::new();
		let mut writer = MemStream::new();
		while expected.len() < BLOCK_SIZE * 3 {
			let mut block = Vec::with_capacity(write_size);
			for _ in 0..write_size {
				block.push(rand::random::<u8>());
			}
			assert_eq!(writer.write(&block).unwrap(), write_size);
			assert_eq!(expected.write(&block).unwrap(), write_size);
		}
		{
			let mut actual = Vec::new();
			writer.copy(&mut actual).unwrap();
			assert_eq!(expected.len(), actual.len());
			assert_eq!(expected, actual);
		}
		{
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
