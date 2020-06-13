use std::env;
use std::io;
use std::io::{Error, Read};
use std::iter::FromIterator;
use std::time::Instant;

use sha2::{Digest, Sha256};

pub const DEFAULT_BUF_SIZE: usize = 1024 * 64;

pub fn filter<T, R, F: Fn(&T) -> Option<R>>(args: &[T], filter: F) -> Vec<R> {
    Vec::from_iter(args.iter().filter_map(filter))
}

pub fn hash_stream<R: Read>(reader: &mut R) -> Result<String, Error> {
    let mut hasher = Sha256::new();
    io::copy(reader, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
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
