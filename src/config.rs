extern crate num_cpus;

use std::env;
use std::io::Result;
use std::path::{Path, PathBuf};

pub struct Config {
    pub process_limit: usize,
    pub cache_dir: PathBuf,
    pub cache_limit: u64,
}

impl Config {
	pub fn new() -> Result<Self> {
		let cache_dir = match env::var("OCTOBUILD_CACHE") {
			Ok(value) => Path::new(&value).to_path_buf(),
			Err(_) => env::home_dir().unwrap().join(".octobuild").join("cache")
		};
		Ok(Config {
			process_limit: num_cpus::get(),
			cache_dir: cache_dir,
			cache_limit: 16 * 1024 * 1024 * 1024,
		})
	}
}