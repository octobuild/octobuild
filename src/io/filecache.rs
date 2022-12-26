use std::cmp::min;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::super::compiler::OutputInfo;
use super::super::config::Config;
use super::super::utils::DEFAULT_BUF_SIZE;
use super::binary::*;
use super::counter::Counter;
use super::statistic::Statistic;
use std::time::SystemTime;

const HEADER: &[u8] = b"OBCF\x00\x03";
const FOOTER: &[u8] = b"END\x00";
const SUFFIX: &str = ".lz4";

#[derive(Debug)]
pub enum CacheError {
    InvalidHeader(PathBuf),
    InvalidFooter(PathBuf),
    PackedFilesMismatch(PathBuf),
    MutexError(String),
}

impl Display for CacheError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
        match self {
            CacheError::InvalidHeader(ref path) => {
                write!(f, "invalid cache file header: {}", path.display())
            }
            CacheError::InvalidFooter(ref path) => {
                write!(f, "invalid cache file footer: {}", path.display())
            }
            CacheError::PackedFilesMismatch(ref path) => write!(
                f,
                "unexpected count of packed cached files: {}",
                path.display()
            ),
            CacheError::MutexError(ref message) => write!(f, "mutex error: {}", message),
        }
    }
}

impl ::std::error::Error for CacheError {
    fn description(&self) -> &str {
        match self {
            CacheError::InvalidHeader(_) => "invalid cache file header",
            CacheError::InvalidFooter(_) => "invalid cache file footer",
            CacheError::PackedFilesMismatch(_) => "unexpected count of packed cached files",
            CacheError::MutexError(_) => "mutex error",
        }
    }

    fn cause(&self) -> Option<&dyn (::std::error::Error)> {
        None
    }
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
    pub fn new(config: &Config) -> Self {
        FileCache {
            cache_dir: config.cache_dir.clone(),
            cache_limit: u64::from(config.cache_limit_mb) * 1024 * 1024,
        }
    }

    pub fn run_cached<F: FnOnce() -> Result<OutputInfo, Error>, C: Fn() -> bool>(
        &self,
        statistic: &Statistic,
        hash: &str,
        outputs: &[PathBuf],
        worker: F,
        checker: C,
    ) -> Result<OutputInfo, Error> {
        let path = self
            .cache_dir
            .join(&hash[0..2])
            .join(hash[2..].to_string() + SUFFIX);
        // Try to read data from cache.
        if let Ok(output) = read_cache(statistic, &path, outputs) {
            return Ok(output);
        }
        // Run task and save result to cache.
        let output = worker()?;
        if checker() {
            write_cache(statistic, &path, outputs, &output)?;
        }
        Ok(output)
    }

    pub fn cleanup(&self) -> Result<(), Error> {
        let mut files = find_cache_files(&self.cache_dir, Vec::new())?;
        files.sort_by(|a, b| b.accessed.cmp(&a.accessed));

        let mut cache_size: u64 = 0;
        for item in files.into_iter() {
            cache_size += item.size;
            if cache_size > self.cache_limit {
                fs::remove_file(&item.path)?;
            }
        }
        Ok(())
    }
}

fn find_cache_files(dir: &Path, mut files: Vec<CacheFile>) -> Result<Vec<CacheFile>, Error> {
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

fn write_cached_file<W: Write>(stream: &mut W, path: &Path) -> Result<(), Error> {
    let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
    let mut file = File::open(path)?;
    let total_size = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    write_u64(stream, total_size)?;
    let mut need_size = total_size;
    loop {
        let size = file.read(&mut buf)?;
        if size == 0 && need_size == 0 {
            break;
        }
        if size == 0 {
            return Err(Error::new(
                ErrorKind::BrokenPipe,
                "Unexpected end of stream",
            ));
        }
        if need_size < size as u64 {
            return Err(Error::new(ErrorKind::BrokenPipe, "Expected end of stream"));
        }
        stream.write_all(&buf[0..size])?;
        need_size -= size as u64;
    }
    Ok(())
}

fn write_cache(
    statistic: &Statistic,
    path: &Path,
    paths: &[PathBuf],
    output: &OutputInfo,
) -> Result<(), Error> {
    if !output.success() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?
    }
    let mut stream = lz4::EncoderBuilder::new()
        .level(1)
        .build(Counter::writer(File::create(path)?))?;
    stream.write_all(HEADER)?;
    write_usize(&mut stream, paths.len())?;
    for path in paths.iter() {
        write_cached_file(&mut stream, path)?;
    }
    write_output(&mut stream, output)?;
    stream.write_all(FOOTER)?;
    let (writer, result) = stream.finish();
    statistic.add_miss(writer.len());
    result
}

fn read_cached_file<R: Read>(stream: &mut R, path: &Path) -> Result<(), Error> {
    let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
    let total_size = read_u64(stream)?;
    let mut need_size = total_size;

    let mut file = File::create(path)?;
    file.set_len(total_size)?;
    while need_size > 0 {
        let need = min(buf.len() as u64, need_size) as usize;
        let size = stream.read(&mut buf[0..need])?;
        if size == 0 {
            return Err(Error::new(ErrorKind::BrokenPipe, "Expected end of stream"));
        }
        file.write_all(&buf[0..size])?;
        need_size -= size as u64;
    }
    Ok(())
}

fn read_cache(statistic: &Statistic, path: &Path, paths: &[PathBuf]) -> Result<OutputInfo, Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(Path::new(path))?;
    file.write_all(&[4])?;
    file.seek(SeekFrom::Start(0))?;
    let mut stream = lz4::Decoder::new(Counter::reader(file))?;
    if read_exact(&mut stream, HEADER.len())? != HEADER {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            CacheError::InvalidHeader(path.to_path_buf()),
        ));
    }
    if read_usize(&mut stream)? != paths.len() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            CacheError::PackedFilesMismatch(path.to_path_buf()),
        ));
    }
    for path in paths.iter() {
        let mut temp_name = OsString::from("~tmp~");
        temp_name.push(path.file_name().unwrap());
        let temp = path.with_file_name(temp_name);
        drop(fs::remove_file(path));
        match read_cached_file(&mut stream, &temp).and_then(|_| fs::rename(&temp, path)) {
            Ok(_) => {}
            Err(e) => {
                drop(fs::remove_file(&temp));
                return Err(e);
            }
        };
    }
    let output = read_output(&mut stream)?;
    if read_exact(&mut stream, FOOTER.len())? != FOOTER {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            CacheError::InvalidFooter(path.to_path_buf()),
        ));
    }
    let mut eof = [0];
    if stream.read(&mut eof)? != 0 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            CacheError::InvalidFooter(path.to_path_buf()),
        ));
    }
    statistic.add_hit(stream.finish().0.len());
    Ok(output)
}

fn write_blob(stream: &mut dyn Write, blob: &[u8]) -> Result<(), Error> {
    write_usize(stream, blob.len())?;
    stream.write_all(blob)?;
    Ok(())
}

fn read_blob(stream: &mut dyn Read) -> Result<Vec<u8>, Error> {
    let size = read_usize(stream)?;
    read_exact(stream, size)
}

fn write_output(stream: &mut dyn Write, output: &OutputInfo) -> Result<(), Error> {
    write_blob(stream, &output.stdout)?;
    write_blob(stream, &output.stderr)?;
    Ok(())
}

fn read_output(stream: &mut dyn Read) -> Result<OutputInfo, Error> {
    let stdout = read_blob(stream)?;
    let stderr = read_blob(stream)?;
    Ok(OutputInfo {
        status: Some(0),
        stdout,
        stderr,
    })
}
