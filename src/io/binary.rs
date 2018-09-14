use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem;

/// Reads a single byte. Returns `Err` on EOF.
fn read_byte(stream: &mut Read) -> Result<u8> {
    let mut buf = [0];
    let size = stream.read(&mut buf)?;
    if size <= 0 {
        return Err(Error::new(ErrorKind::InvalidInput, "Unexpected end of data"));
    }
    Ok(buf[0])
}

/// Reads exactly `len` bytes and gives you back a new vector of length
/// `len`
///
/// # Error
///
/// Fails with the same conditions as `read`. Additionally returns error
/// on EOF. Note that if an error is returned, then some number of bytes may
/// have already been consumed from the underlying reader, and they are lost
/// (not returned as part of the error). If this is unacceptable, then it is
/// recommended to use the `push_at_least` or `read` methods.
pub fn read_exact(stream: &mut Read, len: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(len);
    unsafe {
        buf.set_len(len);
    }
    read_array(stream, &mut buf[..])?;
    Ok(buf)
}

fn read_array(stream: &mut Read, buf: &mut [u8]) -> Result<()> {
    let mut pos = 0;
    while pos < buf.len() {
        let size = stream.read(&mut buf[pos..])?;
        if size <= 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "Unexpected end of data"));
        }
        pos += size;
    }
    Ok(())
}

#[inline]
pub fn write_u8(stream: &mut Write, n: u8) -> Result<()> {
    stream.write_all(&[n])
}

#[inline]
pub fn write_u64(stream: &mut Write, i: u64) -> Result<()> {
    stream.write_all(&unsafe { mem::transmute::<_, [u8; 8]>(i) })
}

#[inline]
pub fn write_usize(stream: &mut Write, i: usize) -> Result<()> {
    write_u64(stream, i as u64)
}

#[inline]
pub fn read_u8(stream: &mut Read) -> Result<u8> {
    read_byte(stream)
}

#[inline]
pub fn read_u64(stream: &mut Read) -> Result<u64> {
    let mut buf: [u8; 8] = [0; 8];
    read_array(stream, &mut buf)?;
    Ok(unsafe { mem::transmute_copy::<_, u64>(&buf) })
}

#[inline]
pub fn read_usize(stream: &mut Read) -> Result<usize> {
    read_u64(stream).map(|i| i as usize)
}
