use std::hash;
use std::hash::{Hasher, SipHasher};
use std::old_io::{IoError, IoErrorKind, Reader};

pub const DEFAULT_BUF_SIZE: usize = 1024 * 64;

pub fn filter<T, R, F:Fn(&T) -> Option<R>>(args: &Vec<T>, filter:F) -> Vec<R> {
	let mut result: Vec<R> = Vec::new();
	for arg in args.iter() {
		match filter(arg) {
			Some(v) => {
				result.push(v);
			}
			None => {}
		}
	}
	result
}

pub fn hash_text(data: &[u8]) -> String {
	let mut hash = SipHasher::new();
	hash.write(data);
	format!("{:016x}", hash.finish())
}

pub fn hash_stream(stream: &mut Reader) -> Result<String, IoError> {
	let mut hash = SipHasher::new();
	try! (hash_write_stream(&mut hash, stream));
	Ok(format!("{:016x}", hash.result()))
}

pub fn hash_write_stream(hash: &mut Hasher, stream: &mut Reader) -> Result<(), IoError> {
	let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
	loop {
		match stream.read(&mut buf) {
			Ok(size) => {
				hash.write(&buf.as_slice()[0..size]);
			}
			Err(ref e) if e.kind == IoErrorKind::EndOfFile => break,
			Err(e) => return Err(e)
		}
	}
	Ok(())
}
