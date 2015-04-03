extern crate lz4;

use std::env;
use std::fs;
use std::fs::{File, PathExt, OpenOptions};
use std::io::{Error, ErrorKind, Read, Write, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{Hasher, SipHasher};
use std::path::{Path, PathBuf};

use super::compiler::OutputInfo;
use super::utils::hash_write_stream;
use super::utils::DEFAULT_BUF_SIZE;
use super::io::binary::*;

const HEADER: &'static [u8] = b"OBCF\x00\x02";
const FOOTER: &'static [u8] = b"END\x00";
const SUFFIX: &'static str = ".lz4";

struct FileHash {
	hash: String,
	size: u64,
	modified: u64,
}

#[derive(Clone)]
pub struct Cache {
	file_hash: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<Option<FileHash>>>>>>,
	cache_dir: PathBuf
}

struct CacheFile {
	path: PathBuf,
	size: u64,
	accessed: u64,
}

impl Cache {
	pub fn new() -> Self {
		let cache_dir = match env::var("OCTOBUILD_CACHE") {
			Ok(value) => Path::new(&value).to_path_buf(),
			Err(_) => env::home_dir().unwrap().join(".octobuild").join("cache")
		};
		Cache {
			file_hash: Arc::new(Mutex::new(HashMap::new())),
			cache_dir: cache_dir
		}
	}

	pub fn run_cached<F: Fn()->Result<OutputInfo, Error>>(&self, params: &str, inputs: &Vec<PathBuf>, outputs: &Vec<PathBuf>, worker: F) -> Result<OutputInfo, Error> {
		let hash = try! (self.generate_hash(params, inputs));
		let path = self.cache_dir.join(&hash[0..2]).join(&(hash[2..].to_string() + SUFFIX));
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

	pub fn cleanup(&self, max_cache_size: u64) -> Result<(), Error> {
		let mut files: Vec<CacheFile> = Vec::new();
		for item in try! (fs::walk_dir(&self.cache_dir)) {
			let path = try! (item).path();
			if path.to_str().map_or(false, |v| v.ends_with(SUFFIX)) {
				let attr = try! (fs::metadata(&path));
				if attr.is_file() {
					files.push(CacheFile {
						path: path,
						size: attr.len(),
						accessed: attr.modified(),
					});
				}
			}
		}
		files.as_mut_slice().sort_by(|a, b| b.accessed.cmp(&a.accessed));
		
		let mut cache_size: u64 = 0;
		for item in files.into_iter() {
			cache_size += item.size;
			if cache_size > max_cache_size {
				let _ = try!(fs::remove_file(&item.path));
			}
		}
		Ok(())
	}

	fn generate_hash(&self, params: &str, inputs: &Vec<PathBuf>) -> Result<String, Error> {
		let mut sip_hash = SipHasher::new();
		let hash: &mut Hasher = &mut sip_hash;
		// str
		hash.write(params.as_bytes());
		hash.write_u8(0);
		// inputs
		for input in inputs.iter() {
			let file_hash = try! (self.get_file_hash(input));
			hash.write(file_hash.as_bytes());
		}
		Ok(format!("{:016x}", hash.finish()))
	}

	pub fn get_file_hash(&self, path: &Path) -> Result<String, Error> {
		// Get/create lock for file entry.
		let hash_lock = match self.file_hash.lock() {
			Ok(mut map) => {
				match map.entry(path.to_path_buf()) {
					Entry::Occupied(entry) => entry.get().clone(),
					Entry::Vacant(entry) => entry.insert(Arc::new(Mutex::new(None))).clone()
				}
			}
			Err(e) => {
				return Err(Error::new(ErrorKind::Other, "Mutex error", Some(e.to_string())));
			}
		};
		// Get file hash.
		let result = match hash_lock.lock() { // rust: #22722
			Ok(mut hash_entry) => {
				// Validate entry, if exists.
				match *hash_entry {
					Some(ref value) => {
						let stat = try! (fs::metadata(path));
						if value.size == stat.len() && value.modified == stat.modified() {
							return Ok(value.hash.clone());
						}
					}
					None => {}
				}
				// Calculate hash value.
				let stat = try! (fs::metadata(path));
				let hash = try! (generate_file_hash(path));
				*hash_entry = Some(FileHash {
					hash: hash.clone(),
					size: stat.len(),
					modified: stat.modified(),
				});
				Ok(hash)
			}
			Err(e) => Err(Error::new(ErrorKind::Other, "Mutex error", Some(e.to_string())))
		};
		result
	}
}

fn generate_file_hash(path: &Path) -> Result<String, Error> {
	let mut hash = SipHasher::new();
	let mut file = try! (File::open(path));
	try! (hash_write_stream(&mut hash, &mut file));
	Ok(format!("{:016x}", hash.finish()))
}

fn write_cache(path: &Path, paths: &Vec<PathBuf>, output: &OutputInfo) -> Result<(), Error> {
	if !output.success() {
		return Ok(());
	}
	match path.parent() {
		Some(parent) => try! (fs::create_dir_all(&parent)),
		None => ()
	}
	let mut stream = try! (lz4::Encoder::new(try! (File::create(path)), 1));
	try! (stream.write_all(HEADER));
	try! (write_le_usize(&mut stream, paths.len()));
	let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
	for path in paths.iter() {
		let mut file = try! (File::open(path));
		loop {
			let size = try! (file.read(&mut buf));
			if size <= 0 {
				break;
			}
			try! (write_le_usize(&mut stream, size));
			try! (stream.write_all(&buf[0..size]));
		}
		try! (write_le_usize(&mut stream, 0));
	}
	try! (write_output(&mut stream, output));
	try! (stream.write_all(FOOTER));
	match stream.finish() {
		(_, result) => result
	}
}

fn read_cache(path: &Path, paths: &Vec<PathBuf>) -> Result<OutputInfo, Error> {
	let mut file = try! (OpenOptions::new().read(true).write(true).open(Path::new(path)));
	try! (file.write(&[4]));
	try! (file.seek(SeekFrom::Start(0)));
	let mut stream = try! (lz4::Decoder::new (file));
	if try! (read_exact(&mut stream, HEADER.len())) != HEADER {
		return Err(Error::new(ErrorKind::InvalidInput, "Invalid cache file header", Some(path.display().to_string())));
	}
	if try! (read_le_usize(&mut stream)) != paths.len() {
		return Err(Error::new(ErrorKind::InvalidInput, "Unexpected count of packed cached files", Some(path.display().to_string())));
	} 
	for path in paths.iter() {
		let mut file = try! (File::create(path));
		loop {
			let size = try! (read_le_usize(&mut stream));
			if size == 0 {break;}
			let block = try! (read_exact(&mut stream, size));
			try! (file.write_all(&block));
		}
	}
	let output = try! (read_output(&mut stream));
	if try! (read_exact(&mut stream, FOOTER.len())) != FOOTER {
		return Err(Error::new(ErrorKind::InvalidInput, "Invalid cache file footer", Some(path.display().to_string())));
	}
	Ok(output)
}

fn write_blob(stream: &mut Write, blob: &[u8]) -> Result<(), Error> {
	try! (write_le_usize(stream, blob.len()));
	try! (stream.write_all(blob));
	Ok(())
}

fn read_blob(stream: &mut Read) -> Result<Vec<u8>, Error> {
	let size = try! (read_le_usize(stream));
	read_exact(stream, size)
}

fn write_output(stream: &mut Write, output: &OutputInfo) -> Result<(), Error> {
	try! (write_blob(stream, &output.stdout));
	try! (write_blob(stream, &output.stderr));
	Ok(())
}

fn read_output(stream: &mut Read) -> Result<OutputInfo, Error> {
	let stdout = try! (read_blob(stream));
	let stderr = try! (read_blob(stream));
	Ok(OutputInfo {
		status: Some(0),
		stdout: stdout,
		stderr: stderr,
	})
}
