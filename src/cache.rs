extern crate "sha1-hasher" as sha1;

use std::os;
use std::io::fs;
use std::io::{File, IoError, IoErrorKind, Reader, Writer, USER_RWX};
use std::io::process::{ProcessOutput, ProcessExit};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

const HEADER: &'static [u8] = b"OBCF\x00\x01";
const FOOTER: &'static [u8] = b"END\x00";
const DEFAULT_BUF_SIZE: usize = 1024 * 64;

struct FileHash {
	hash: String,
	size: u64,
	modified: u64,
}

#[derive(Clone)]
pub struct Cache {
	file_sha1: Arc<Mutex<HashMap<Path, Arc<Mutex<Option<FileHash>>>>>>,
	cache_dir: Path
}

impl Cache {
	pub fn new() -> Self {
		let cache_dir = os::homedir().unwrap().join_many(&[".octobuild", "cache"]);
		Cache {
			file_sha1: Arc::new(Mutex::new(HashMap::new())),
			cache_dir: cache_dir
		}
	}

	pub fn run_cached<F: Fn()->Result<ProcessOutput, IoError>>(&self, params: &str, inputs: &Vec<Path>, outputs: &Vec<Path>, worker: F) -> Result<ProcessOutput, IoError> {
		let hash = try! (self.generate_hash(params, inputs));
		let path = self.cache_dir.join(hash.slice(0, 2)).join(hash.slice(2, 4)).join(hash.slice_from(4));
		// Try to read data from cache.
		match read_cache(&path, outputs) {
			Ok(output) => {return Ok(output)}
			Err(_) => {}
		}
		// Run task and save result to cache.
		let output = try !(worker());
		try !(write_cache(&path, outputs, &output));
		Ok(output)
	}

	fn generate_hash(&self, params: &str, inputs: &Vec<Path>) -> Result<String, IoError> {
		use std::hash::Writer;

		let mut hash = sha1::Sha1::new();
		// str
		hash.write(params.as_bytes());
		hash.write(&[0]);
		// inputs
		for input in inputs.iter() {
			let file_hash = try! (self.get_file_hash(input));
			hash.write(file_hash.as_bytes());
		}
		Ok(hash.hexdigest())
	}

	pub fn get_file_hash(&self, path: &Path) -> Result<String, IoError> {
		// Get/create lock for file entry.
		let hash_lock = match self.file_sha1.lock() {
			Ok(mut map) => {
				match map.entry(path.clone()) {
					Entry::Occupied(entry) => entry.get().clone(),
					Entry::Vacant(entry) => entry.insert(Arc::new(Mutex::new(None))).clone()
				}
			}
			Err(e) => {
				return Err(IoError {
					kind: IoErrorKind::OtherIoError,
					desc: "Mutex error",
					detail: Some(e.to_string())
				})
			}
		};
		// Get file hash.
		match hash_lock.lock() {
			Ok(mut hash_entry) => {
				// Validate entry, if exists.
				match *hash_entry {
					Some(ref value) => {
						let stat = try! (fs::stat(path));
						if value.size == stat.size && value.modified == stat.modified {
							return Ok(value.hash.clone());
						}
					}
					None => {}
				}
				// Calculate hash value.
				let stat = try! (fs::stat(path));
				let hash = try! (generate_file_hash(path));
				*hash_entry = Some(FileHash {
					hash: hash.clone(),
					size: stat.size,
					modified: stat.modified,
				});
				Ok(hash)
			}
			Err(e) => Err(IoError {
				kind: IoErrorKind::OtherIoError,
				desc: "Mutex error",
				detail: Some(e.to_string())
			})
		}
	}
}

fn generate_file_hash(path: &Path) -> Result<String, IoError> {
	use std::hash::Writer;
	let mut hash = sha1::Sha1::new();
	let mut file = try! (File::open(path));
	let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
	loop {
		match file.read(&mut buf) {
			Ok(size) => {
				hash.write(&buf.as_slice()[0..size]);
			}
			Err(ref e) if e.kind == IoErrorKind::EndOfFile => break,
			Err(e) => return Err(e)
		}
	}
	Ok(hash.hexdigest())
}

fn write_cache(path: &Path, paths: &Vec<Path>, output: &ProcessOutput) -> Result<(), IoError> {
	if !output.status.success() {
		return Ok(());
	}
	try! (fs::mkdir_recursive(&path.dir_path(), USER_RWX));
	let mut stream = try! (File::create(path));
	try! (stream.write(HEADER));
	try! (stream.write_le_uint(paths.len()));
	let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
	for path in paths.iter() {
		let mut file = try! (File::open(path));
		loop {
			match file.read(&mut buf) {
				Ok(size) => {
					try! (stream.write_le_uint(size));
					try! (stream.write(&buf.as_slice()[0..size]));
				}
				Err(ref e) if e.kind == IoErrorKind::EndOfFile => break,
				Err(e) => return Err(e)
			}
		}
		try! (stream.write_le_uint(0));
	}
	try! (write_output(&mut stream, output));
	try! (stream.write(FOOTER));
	Ok(())
}

fn read_cache(path: &Path, paths: &Vec<Path>) -> Result<ProcessOutput, IoError> {
	let mut stream = try! (File::open(path));
	if try! (stream.read_exact(HEADER.len())) != HEADER {
		return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Invalid cache file header",
			detail: Some(path.display().to_string())
		})
	}
	if try! (stream.read_le_uint()) != paths.len() {
		return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Unexpected count of packed cached files",
			detail: Some(path.display().to_string())
		})
	} 
	for path in paths.iter() {
		let mut file = try! (File::create(path));
		loop {
			let size = try! (stream.read_le_uint());
			if size == 0 {break;}
			let block = try! (stream.read_exact(size));
			try! (file.write(block.as_slice()));
		}
	}
	let output = try! (read_output(&mut stream));
	if try! (stream.read_exact(FOOTER.len())) != FOOTER {
		return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Invalid cache file footer",
			detail: Some(path.display().to_string())
		})
	}
	Ok(output)
}

fn write_blob(stream: &mut Writer, blob: &[u8]) -> Result<(), IoError> {
	try! (stream.write_le_uint(blob.len()));
	try! (stream.write(blob));
	Ok(())
}

fn read_blob(stream: &mut Reader) -> Result<Vec<u8>, IoError> {
	let size = try! (stream.read_le_uint());
	stream.read_exact(size)
}

fn write_output(stream: &mut Writer, output: &ProcessOutput) -> Result<(), IoError> {
	try! (write_blob(stream, output.output.as_slice()));
	try! (write_blob(stream, output.error.as_slice()));
	Ok(())
}

fn read_output(stream: &mut Reader) -> Result<ProcessOutput, IoError> {
	let output = try! (read_blob(stream));
	let error  = try! (read_blob(stream));
	Ok(ProcessOutput {
		status: ProcessExit::ExitStatus(0),
		output: output,
		error: error,
	})
}
