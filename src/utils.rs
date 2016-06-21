use log;
use md5;
use fern;
use time;
use std::env;
use std::iter::FromIterator;
use std::io::{Error, Read, Write};

pub const DEFAULT_BUF_SIZE: usize = 1024 * 64;

pub fn filter<T, R, F: Fn(&T) -> Option<R>>(args: &Vec<T>, filter: F) -> Vec<R> {
    Vec::from_iter(args.iter().filter_map(filter))
}

pub fn hash_stream<R: Read>(stream: &mut R) -> Result<String, Error> {
    let mut hash = md5::Context::new();
    try!(hash_write_stream(&mut hash, stream));
    Ok(hex_lower(&hash.compute()))
}

pub fn hex_lower(data: &[u8]) -> String {
    let mut hex = String::with_capacity(data.len() * 2);
    for &byte in data.iter() {
        use std::fmt::Write;
        write!(&mut hex, "{:02x}", byte).unwrap();
    }
    hex
}

fn hash_write_stream<W: Write, R: Read>(hash: &mut W, stream: &mut R) -> Result<(), Error> {
    let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
    loop {
        let size = try!(stream.read(&mut buf));
        if size <= 0 {
            break;
        }
        try!(hash.write(&buf[0..size]));
    }
    Ok(())
}

pub fn init_logger() {
    let log_file = env::current_exe().unwrap().with_extension("log");

    // Create a basic logger configuration
    let logger_config = fern::DispatchConfig {
        format: Box::new(|msg, level, _location| {
            // This format just displays [{level}] {message}
            format!("{} [{}] {}", time::now().rfc3339(), level, msg)
        }),
        // Output to stdout and the log file in the temporary directory we made above to test
        output: vec![fern::OutputConfig::stdout(), fern::OutputConfig::file(&log_file)],
        // Only log messages Info and above
        level: log::LogLevelFilter::Info,
    };

    if let Err(e) = fern::init_global_logger(logger_config, log::LogLevelFilter::Trace) {
        panic!("Failed to initialize global logger: {}", e);
    }
}

#[test]
fn test_hex_lower() {
    assert_eq!(hex_lower(&[0x01, 0x02, 0x82, 0xFF]), "010282ff".to_string());
}

#[test]
fn test_hash_stream() {
    use std::io::Cursor;
    assert_eq!(hash_stream(&mut Cursor::new(b"foobar")).unwrap(),
               "3858f62230ac3c915f300c664312c63f".to_string());
}
