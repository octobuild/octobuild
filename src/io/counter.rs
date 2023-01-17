use std::io::{Read, Result, Write};

pub struct Counter<S> {
    stream: S,
    size: usize,
}

impl<S> Counter<S> {
    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

impl<R: Read> Counter<R> {
    pub fn reader(r: R) -> Counter<R> {
        Counter { stream: r, size: 0 }
    }
}

impl<W: Write> Counter<W> {
    pub fn writer(w: W) -> Counter<W> {
        Counter { stream: w, size: 0 }
    }
}

impl<R: Read> Read for Counter<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let size = self.stream.read(buf)?;
        self.size += size;
        Ok(size)
    }
}

impl<W: Write> Write for Counter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let size = self.stream.write(buf)?;
        self.size += size;
        Ok(size)
    }

    fn flush(&mut self) -> Result<()> {
        self.stream.flush()
    }
}
