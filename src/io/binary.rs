use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Result, Write};

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
#[allow(clippy::uninit_vec)]
pub fn read_exact(stream: &mut impl Read, len: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(len);
    unsafe {
        buf.set_len(len);
    }
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

#[inline]
pub fn write_u64(stream: &mut impl Write, i: u64) -> Result<()> {
    stream.write_u64::<LittleEndian>(i)
}

#[inline]
pub fn write_usize(stream: &mut impl Write, i: usize) -> Result<()> {
    write_u64(stream, i as u64)
}

#[inline]
pub fn read_u64(stream: &mut impl Read) -> Result<u64> {
    stream.read_u64::<LittleEndian>()
}

#[inline]
pub fn read_usize(stream: &mut impl Read) -> Result<usize> {
    Ok(read_u64(stream)? as usize)
}
