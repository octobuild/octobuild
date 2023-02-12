use std::ffi::OsString;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use std::time::SystemTime;

use crate::compiler::OutputInfo;
use crate::config::Config;
use crate::io::binary::{read_exact, read_u64, read_usize, write_u64, write_usize};
use crate::io::counter::Counter;
use crate::io::statistic::Statistic;
use thiserror::Error;

const HEADER: &[u8] = b"OBCF\x00\x03";
const FOOTER: &[u8] = b"END\x00";
const SUFFIX: &str = ".lz4";

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("invalid cache file header: {0}")]
    InvalidHeader(PathBuf),
    #[error("invalid cache file footer: {0}")]
    InvalidFooter(PathBuf),
    #[error("unexpected count of packed cached files: {0}")]
    PackedFilesMismatch(PathBuf),
    #[error("mutex error: {0}")]
    MutexError(String),
}

pub struct FileCache {
    cache_dir: PathBuf,
    cache_limit: u64,
}

struct CacheFile {
    path: PathBuf,
    size: u64,
    accessed: SystemTime,
}

impl FileCache {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        FileCache {
            cache_dir: config.cache.clone(),
            cache_limit: config.cache_limit_mb * 1024 * 1024,
        }
    }

    pub fn run_cached<F: FnOnce() -> crate::Result<OutputInfo>>(
        &self,
        statistic: &Statistic,
        hash: &str,
        outputs: Vec<PathBuf>,
        worker: F,
    ) -> crate::Result<OutputInfo> {
        let path = self
            .cache_dir
            .join(&hash[0..2])
            .join(hash[2..].to_string() + SUFFIX);
        // Try to read data from cache.
        if let Ok(output) = read_cache(statistic, &path, &outputs) {
            return Ok(output);
        }
        // Run task and save result to cache.
        let output = worker()?;
        write_cache(statistic, &path, outputs, &output)?;
        Ok(output)
    }

    pub fn cleanup(&self) -> crate::Result<()> {
        let mut files = find_cache_files(&self.cache_dir, Vec::new())?;
        files.sort_by(|a, b| b.accessed.cmp(&a.accessed));

        let mut cache_size: u64 = 0;
        for item in &files {
            cache_size += item.size;
            if cache_size > self.cache_limit {
                fs::remove_file(&item.path)?;
            }
        }
        Ok(())
    }
}

fn find_cache_files(dir: &Path, mut files: Vec<CacheFile>) -> crate::Result<Vec<CacheFile>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let stat = fs::metadata(&path)?;
        if stat.is_dir() {
            let r = find_cache_files(&path, files);
            files = r?;
        } else {
            files.push(CacheFile {
                path,
                size: stat.len(),
                accessed: stat.modified()?,
            });
        }
    }
    Ok(files)
}

fn write_cached_file<W: Write>(stream: &mut W, path: PathBuf) -> crate::Result<()> {
    assert!(path.is_absolute());
    let mut file = File::open(&path).map_err(|e| crate::Error::FileOpen {
        path,
        error: Box::new(e.into()),
    })?;
    let total_size = file.seek(SeekFrom::End(0))?;
    file.rewind()?;
    write_u64(stream, total_size)?;
    let written = std::io::copy(&mut file, stream)?;
    if written != total_size {
        return Err(crate::Error::Generic("Expected end of stream".to_string()));
    }
    Ok(())
}

fn write_cache(
    statistic: &Statistic,
    path: &Path,
    paths: Vec<PathBuf>,
    output: &OutputInfo,
) -> crate::Result<()> {
    if !output.success() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut stream = lz4::EncoderBuilder::new()
        .level(1)
        .build(Counter::writer(File::create(path)?))?;
    stream.write_all(HEADER)?;
    write_usize(&mut stream, paths.len())?;
    for path in paths {
        assert!(path.is_absolute());
        write_cached_file(&mut stream, path)?;
    }
    write_output(&mut stream, output)?;
    stream.write_all(FOOTER)?;
    let (writer, result) = stream.finish();
    statistic.add_miss(writer.len());
    Ok(result?)
}

fn read_cached_file<R: Read>(stream: &mut R, path: &Path) -> crate::Result<()> {
    let size = read_u64(stream)?;
    let mut file = File::create(path)?;
    file.set_len(size)?;
    let written = std::io::copy(&mut stream.take(size), &mut file)?;
    if written != size {
        return Err(crate::Error::Generic("Expected end of stream".to_string()));
    }
    Ok(())
}

fn read_cache(
    statistic: &Statistic,
    path: &PathBuf,
    paths: &[PathBuf],
) -> crate::Result<OutputInfo> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(path))?;
    file.write_all(&[4])?;
    file.rewind()?;
    let mut stream = lz4::Decoder::new(Counter::reader(file))?;
    if read_exact(&mut stream, HEADER.len())? != HEADER {
        return Err(CacheError::InvalidHeader(path.clone()).into());
    }
    if read_usize(&mut stream)? != paths.len() {
        return Err(CacheError::PackedFilesMismatch(path.clone()).into());
    }
    for path in paths {
        assert!(path.is_absolute());
        let mut temp_name = OsString::from("~tmp~");
        temp_name.push(path.file_name().unwrap());
        let temp = path.with_file_name(temp_name);
        drop(fs::remove_file(path));
        match read_cached_file(&mut stream, &temp).and_then(|_| Ok(fs::rename(&temp, path)?)) {
            Ok(_) => {}
            Err(e) => {
                drop(fs::remove_file(&temp));
                return Err(e);
            }
        };
    }
    let output = read_output(&mut stream)?;
    if read_exact(&mut stream, FOOTER.len())? != FOOTER {
        return Err(CacheError::InvalidFooter(path.clone()).into());
    }
    let mut eof = [0];
    if stream.read(&mut eof)? != 0 {
        return Err(CacheError::InvalidFooter(path.clone()).into());
    }
    statistic.add_hit(stream.finish().0.len());
    Ok(output)
}

fn write_blob(stream: &mut dyn Write, blob: &[u8]) -> crate::Result<()> {
    write_usize(stream, blob.len())?;
    stream.write_all(blob)?;
    Ok(())
}

fn read_blob(stream: &mut dyn Read) -> crate::Result<Vec<u8>> {
    let size = read_usize(stream)?;
    Ok(read_exact(stream, size)?)
}

fn write_output(stream: &mut dyn Write, output: &OutputInfo) -> crate::Result<()> {
    write_blob(stream, &output.stdout)?;
    write_blob(stream, &output.stderr)?;
    Ok(())
}

fn read_output(stream: &mut dyn Read) -> crate::Result<OutputInfo> {
    let stdout = read_blob(stream)?;
    let stderr = read_blob(stream)?;
    Ok(OutputInfo {
        status: Some(0),
        stdout,
        stderr,
    })
}
