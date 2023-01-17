use std::cmp::min;
use std::collections::vec_deque;
use std::collections::VecDeque;
use std::io::Result;
pub use std::io::{Read, Write};
use std::mem::MaybeUninit;

const BLOCK_SIZE: usize = 0x10000 - 0x100;

type Block = [u8; BLOCK_SIZE];

#[derive(Default)]
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
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn reader(&self) -> MemReader {
        let mut iter = self.iter();
        let last = iter.next();
        MemReader {
            offset: 0,
            iter,
            last,
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
            writer.write_all(block)?;
        }
        Ok(self.size)
    }

    #[allow(clippy::uninit_assumed_init)]
    #[allow(invalid_value)]
    fn write_data(&mut self, buf: &[u8]) -> usize {
        let mut src_offset = 0;
        while src_offset < buf.len() {
            let dst_offset = self.size % BLOCK_SIZE;
            if dst_offset == 0 {
                self.blocks
                    .push_back(unsafe { MaybeUninit::uninit().assume_init() });
            };
            let block = self.blocks.back_mut().unwrap();
            let copy_size = min(buf.len() - src_offset, BLOCK_SIZE - dst_offset);
            block[dst_offset..dst_offset + copy_size]
                .copy_from_slice(&buf[src_offset..src_offset + copy_size]);
            self.size += copy_size;
            src_offset += copy_size;
        }
        src_offset
    }
}

impl<'a> From<&'a MemStream> for Vec<u8> {
    fn from(stream: &'a MemStream) -> Self {
        let mut buffer = Vec::with_capacity(stream.size);
        stream.copy(&mut buffer).unwrap();
        buffer
    }
}

impl From<Vec<u8>> for MemStream {
    fn from(data: Vec<u8>) -> Self {
        let mut result = MemStream::new();
        result.write_data(&data);
        result
    }
}

impl<'a> From<&'a [u8]> for MemStream {
    fn from(data: &[u8]) -> Self {
        let mut result = MemStream::new();
        result.write_data(data);
        result
    }
}

impl Write for MemStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        Ok(self.write_data(buf))
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
                    buf[dst_offset..dst_offset + copy_size]
                        .copy_from_slice(&block[self.offset..self.offset + copy_size]);
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
    use crate::io::memstream::{MemStream, BLOCK_SIZE};
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
            let mut block = vec![0; read_size];
            loop {
                let size = reader.read(&mut block).unwrap();
                if size == 0 {
                    break;
                }
                actual.write_all(&block[0..size]).unwrap();
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
