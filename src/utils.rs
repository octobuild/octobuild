use std::hash::{Hasher, SipHasher};
use std::io::{Error, Read};

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
	let mut sip_hash = SipHasher::new();
	let hash: &mut Hasher = &mut sip_hash;
	hash.write(data);
	format!("{:016x}", hash.finish())
}

pub fn hash_stream(stream: &mut Read) -> Result<String, Error> {
	let mut hash = SipHasher::new();
	try! (hash_write_stream(&mut hash, stream));
	Ok(format!("{:016x}", hash.finish()))
}

pub fn hash_write_stream(hash: &mut Hasher, stream: &mut Read) -> Result<(), Error> {
	let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
	loop {
		let size = try! (stream.read(&mut buf));
		if size <= 0 {
			break;
		}
		hash.write(&buf.as_slice()[0..size]);
	}
	Ok(())
}
