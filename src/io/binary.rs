use std;
use std::old_io::extensions;
use std::io::{Error, ErrorKind, Read, Write, Result};

/// Reads a single byte. Returns `Err` on EOF.
fn read_byte(stream: &mut Read) -> Result<u8> {
	let mut buf = [0];
	let size = try! (stream.read(&mut buf));
	if size <= 0 {
		return Err(Error::new(ErrorKind::InvalidInput, "Unexpected end of data", None));
	}
	Ok(buf[0])
}

/// Reads `n` little-endian unsigned integer bytes.
///
/// `n` must be between 1 and 8, inclusive.
fn read_le_uint_n(stream: &mut Read, nbytes: u32) -> Result<u64> {
    assert!(nbytes > 0 && nbytes <= 8);

    let mut val = 0u64;
    let mut pos = 0;
    let mut i = nbytes;
    while i > 0 {
        val += (try!(read_u8(stream)) as u64) << pos;
        pos += 8;
        i -= 1;
    }
    Ok(val)
}

/// Read a u8.
///
/// `u8`s are 1 byte.
pub fn read_u8(stream: &mut Read) -> Result<u8> {
    read_byte(stream)
}

/// Read an i8.
///
/// `i8`s are 1 byte.
pub fn read_i8(stream: &mut Read) -> Result<i8> {
    read_byte(stream).map(|i| i as i8)
}

/// Reads a little-endian unsigned integer.
///
/// The number of bytes returned is system-dependent.
pub fn read_le_usize(stream: &mut Read) -> Result<usize> {
    read_le_uint_n(stream, std::usize::BYTES).map(|i| i as usize)
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
    let mut pos = 0;
    while pos < len {
        let size = try! (stream.read(&mut buf[pos..len]));
        if size <= 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "Unexpected end of data", None));
        }
        pos += size;

    }
    Ok(buf)
}

/// Write a u8 (1 byte).
#[inline]
pub fn write_u8(stream: &mut Write, n: u8) -> Result<()> {
    stream.write_all(&[n])
}

/// Write an i8 (1 byte).
#[inline]
pub fn write_i8(stream: &mut Write, n: i8) -> Result<()> {
    stream.write_all(&[n as u8])
}

/// Write a little-endian uint (number of bytes depends on system).
#[inline]
pub fn write_le_usize(stream: &mut Write, n: usize) -> Result<()> {
    extensions::u64_to_le_bytes(n as u64, std::usize::BYTES as usize, |v| stream.write_all(v))
}
