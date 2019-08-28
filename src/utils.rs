use crypto::digest::Digest;
use crypto::md5::Md5;
use fern;
use log;
use std::env;
use std::io;
use std::io::{Error, Read};
use std::iter::FromIterator;
use time;

pub const DEFAULT_BUF_SIZE: usize = 1024 * 64;

pub fn filter<T, R, F: Fn(&T) -> Option<R>>(args: &[T], filter: F) -> Vec<R> {
    Vec::from_iter(args.iter().filter_map(filter))
}

pub fn hash_stream<R: Read>(reader: &mut R) -> Result<String, Error> {
    let mut hash = Md5::new();
    io::copy(reader, &mut hash.as_write())?;
    Ok(hash.result_str())
}

pub fn init_logger() {
    let log_file = env::current_exe().unwrap().with_extension("log");

    // Create a basic logger configuration
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                time::now().rfc3339(),
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
        "3858f62230ac3c915f300c664312c63f".to_string()
    );
}
