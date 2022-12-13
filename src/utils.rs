use std::io;
use std::io::{Error, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{env, fs};

use crate::cmd;
use sha2::{Digest, Sha256};

pub const DEFAULT_BUF_SIZE: usize = 1024 * 64;

pub fn filter<T, R, F: Fn(&T) -> Option<R>>(args: &[T], filter: F) -> Vec<R> {
    args.iter().filter_map(filter).collect()
}

pub fn hash_stream<R: Read>(reader: &mut R) -> Result<String, Error> {
    let mut hasher = Sha256::new();
    io::copy(reader, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn expands_response_files(
    base: &Option<PathBuf>,
    args: &[String],
) -> Result<Vec<String>, Error> {
    let mut result = Vec::<String>::new();

    for item in args {
        if !(item.as_ref().starts_with('@')) {
            result.push(item.as_ref().to_string());
            continue;
        }

        let path = match base {
            Some(ref p) => p.join(&item.as_ref()[1..]),
            None => Path::new(&item.as_ref()[1..]).to_path_buf(),
        };
        let data = fs::read(path)?;
        let text = decode_string(&data)?;
        let mut args = cmd::native::parse(&text)?;
        result.append(&mut args);
    }

    Ok(result)
}

pub fn init_logger() {
    let log_file = env::current_exe().unwrap().with_extension("log");

    // Create a basic logger configuration
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{:?} [{}] {}",
                Instant::now(),
                record.level(),
                message
            ))
        })
        // Output to stdout and the log file in the temporary directory we made above to test
        .chain(io::stdout())
        .chain(fern::log_file(&log_file).unwrap())
        // Only log messages Info and above
        .level(log::LevelFilter::Info)
        .apply()
        .expect("Failed to initialize logging");
}

#[test]
fn test_hash_stream() {
    use std::io::Cursor;
    assert_eq!(
        hash_stream(&mut Cursor::new(b"foobar")).unwrap(),
        "c3ab8ff13720e8ad9047dd39466b3c8974e592c2fa383d4a3960714caef0c4f2".to_string()
    );
}
