use log;
use crypto::digest::Digest;
use crypto::md5::Md5;
use fern;
use time;
use std::env;
use std::iter::FromIterator;
use std::io::{Error, Read};

pub const DEFAULT_BUF_SIZE: usize = 1024 * 64;

pub fn filter<T, R, F: Fn(&T) -> Option<R>>(args: &Vec<T>, filter: F) -> Vec<R> {
    Vec::from_iter(args.iter().filter_map(filter))
}

pub fn hash_stream<R: Read>(stream: &mut R) -> Result<String, Error> {
    let mut hash = Md5::new();
    let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
    loop {
        let size = try!(stream.read(&mut buf));
        if size <= 0 {
            break;
        }
        hash.input(&buf[0..size]);
    }
    Ok(hash.result_str())
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
fn test_hash_stream() {
    use std::io::Cursor;
    assert_eq!(hash_stream(&mut Cursor::new(b"foobar")).unwrap(),
               "3858f62230ac3c915f300c664312c63f".to_string());
}
