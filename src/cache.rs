extern crate filetime;

use std::hash::{Hash, Hasher, SipHasher};
use std::path::{Path, PathBuf};
use std::fs;
use std::fs::File;
use std::io::{Error, ErrorKind};

use self::filetime::FileTime;

use super::compiler::OutputInfo;
use super::io::memcache::MemCache;
use super::io::filecache::FileCache;
use super::utils::hash_write_stream;

#[derive(Clone)]
pub struct Cache {
	file_cache: FileCache,
	file_hash_cache: MemCache<PathBuf, Result<FileHash, ()>>,
}

#[derive(Clone)]
struct FileHash {
	hash: String,
	size: u64,
	modified: FileTime,
}

pub trait FileHasher {
	fn file_hash(&self, &Path) -> Result<String, Error>;
}

impl Cache {
	pub fn new() -> Self {
		Cache {
			file_cache: FileCache::new(),
			file_hash_cache: MemCache::new(),
		}
	}
	
	pub fn run_file_cached<F: Fn()->Result<OutputInfo, Error>, C: Fn()->bool>(&self, hash: u64, inputs: &Vec<PathBuf>, outputs: &Vec<PathBuf>, worker: F, checker: C) -> Result<OutputInfo, Error> {
		self.file_cache.run_cached(self, hash, inputs, outputs, worker, checker)
	}
	
	pub fn cleanup(&self, max_cache_size: u64) -> Result<(), Error> {
		self.file_cache.cleanup(max_cache_size)
	}
}

impl FileHasher for Cache {
	fn file_hash(&self, path: &Path) -> Result<String, Error> {
		let result = self.file_hash_cache.run_cached(path.to_path_buf(), |cached: Option<Result<FileHash, ()>>| -> Result<FileHash, ()> {
			let stat = match fs::metadata(path) {
				Ok(value) => value,
				Err(_) => {return Err(());},
			};
			// Validate cached value.
			match cached {
				Some(result) => {
					match result {
						Ok(value) => {
							if value.size == stat.len() && value.modified == FileTime::from_last_modification_time(&stat) {
								return Ok(value);
							}
						}
						Err(_) => {}
					}
				}
				None => {}
			}
			// Calculate hash value.
			let hash = match generate_file_hash(path) {
				Ok(value) => value,
				Err(_) => {return Err(());},
			};
			Ok(FileHash {
				hash: hash.clone(),
				size: stat.len(),
				modified: FileTime::from_last_modification_time(&stat),
			})
		});
		match result {
			Ok(value) => Ok(value.hash),
			Err(_) => Err(Error::new(ErrorKind::Other, "I/O Error")),
		}
	}
}

fn generate_file_hash(path: &Path) -> Result<String, Error> {
	let mut hash = SipHasher::new();
	let mut file = try! (File::open(path));
	try! (hash_write_stream(&mut hash, &mut file));
	Ok(format!("{:016x}", hash.finish()))
}
