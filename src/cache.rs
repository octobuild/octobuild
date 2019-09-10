use std::fs;
use std::fs::File;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use super::compiler::OutputInfo;
use super::config::Config;
use super::io::filecache::FileCache;
use super::io::memcache::MemCache;
use super::io::statistic::Statistic;
use super::utils::hash_stream;

use filetime::FileTime;

pub struct Cache {
    file_cache: FileCache,
    file_hash_cache: MemCache<PathBuf, Result<FileHash, ()>>,
}

#[derive(Clone)]
pub struct FileHash {
    pub hash: String,
    pub size: u64,
    pub modified: FileTime,
}

pub trait FileHasher {
    fn file_hash(&self, path: &Path) -> Result<FileHash, Error>;
}

impl Cache {
    pub fn new(config: &Config) -> Self {
        Cache {
            file_cache: FileCache::new(config),
            file_hash_cache: Default::default(),
        }
    }

    pub fn run_file_cached<F: FnOnce() -> Result<OutputInfo, Error>, C: Fn() -> bool>(
        &self,
        statistic: &Statistic,
        hash: &str,
        outputs: &[PathBuf],
        worker: F,
        checker: C,
    ) -> Result<OutputInfo, Error> {
        self.file_cache
            .run_cached(statistic, hash, outputs, worker, checker)
    }

    pub fn cleanup(&self) -> Result<(), Error> {
        self.file_cache.cleanup()
    }
}

impl FileHasher for Cache {
    fn file_hash(&self, path: &Path) -> Result<FileHash, Error> {
        let result = self.file_hash_cache.run_cached(
            path.to_path_buf(),
            |cached: Option<Result<FileHash, ()>>| -> Result<FileHash, ()> {
                let stat = match fs::metadata(path) {
                    Ok(value) => value,
                    Err(_) => {
                        return Err(());
                    }
                };
                // Validate cached value.
                if let Some(result) = cached {
                    if let Ok(value) = result {
                        if value.size == stat.len()
                            && value.modified == FileTime::from_last_modification_time(&stat)
                        {
                            return Ok(value);
                        }
                    }
                }
                // Calculate hash value.
                let hash = match generate_file_hash(path) {
                    Ok(value) => value,
                    Err(_) => {
                        return Err(());
                    }
                };
                Ok(FileHash {
                    hash,
                    size: stat.len(),
                    modified: FileTime::from_last_modification_time(&stat),
                })
            },
        );
        match result {
            Ok(value) => Ok(value.clone()),
            Err(_) => Err(Error::new(ErrorKind::Other, "I/O Error")),
        }
    }
}

fn generate_file_hash(path: &Path) -> Result<String, Error> {
    File::open(path).and_then(|mut file| hash_stream(&mut file))
}
