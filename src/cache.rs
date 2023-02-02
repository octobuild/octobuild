use crate::compiler::OutputInfo;
use crate::config::Config;
use crate::io::filecache::FileCache;
use crate::io::memcache::MemCache;
use crate::io::statistic::Statistic;
use crate::utils::hash_stream;
use std::fs;
use std::fs::File;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone)]
struct CacheError {
    error_msg: String,
}

pub struct Cache {
    file_cache: FileCache,
    file_hash_cache: MemCache<PathBuf, Result<FileHash, CacheError>>,
}

#[derive(Clone)]
pub struct FileHash {
    pub hash: String,
    pub size: u64,
    pub modified: SystemTime,
}

pub trait FileHasher {
    fn file_hash(&self, path: &Path) -> Result<FileHash, Error>;
}

impl Cache {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Cache {
            file_cache: FileCache::new(config),
            file_hash_cache: MemCache::default(),
        }
    }

    pub fn run_file_cached<F: FnOnce() -> crate::Result<OutputInfo>>(
        &self,
        statistic: &Statistic,
        hash: &str,
        outputs: &[PathBuf],
        worker: F,
    ) -> crate::Result<OutputInfo> {
        self.file_cache.run_cached(statistic, hash, outputs, worker)
    }

    pub fn cleanup(&self) -> crate::Result<()> {
        self.file_cache.cleanup()
    }
}

fn file_hash_helper(
    path: &Path,
    cached: Option<Result<FileHash, CacheError>>,
) -> Result<FileHash, Error> {
    let stat = fs::metadata(path)?;
    let modified = stat.modified()?;
    // Validate cached value.
    if let Some(Ok(value)) = cached {
        if value.size == stat.len() && value.modified == modified {
            return Ok(value);
        }
    }
    let mut file = File::open(path)?;
    let hash = hash_stream(&mut file)?;
    Ok(FileHash {
        hash,
        size: stat.len(),
        modified,
    })
}

impl FileHasher for Cache {
    fn file_hash(&self, path: &Path) -> Result<FileHash, Error> {
        self.file_hash_cache
            .run_cached(
                path.to_path_buf(),
                |cached: Option<Result<FileHash, CacheError>>| -> Result<FileHash, CacheError> {
                    file_hash_helper(path, cached).map_err(|e| CacheError {
                        error_msg: e.to_string(),
                    })
                },
            )
            .map_err(|e| Error::new(ErrorKind::Other, e.error_msg))
    }
}
