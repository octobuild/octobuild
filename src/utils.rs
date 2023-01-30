use local_encoding_ng::{Encoder, Encoding};
use std::ffi::{OsStr, OsString};
use std::io;
use std::io::{Error, Read};
use std::path::PathBuf;
use std::time::Instant;
use std::{env, fs};

use crate::cmd;
use sha2::{Digest, Sha256};

pub const DEFAULT_BUF_SIZE: usize = 1024 * 64;

pub fn hash_stream<R: Read>(reader: &mut R) -> Result<String, Error> {
    let mut hasher = Sha256::new();
    io::copy(reader, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn expand_response_files(
    base: &Option<PathBuf>,
    args: &[String],
) -> crate::Result<Vec<String>> {
    let mut result = Vec::<String>::new();

    for item in args {
        if !(item.starts_with('@')) {
            result.push(item.to_string());
            continue;
        }

        let path = match &base {
            Some(p) => p.join(&item[1..]),
            None => PathBuf::from(&item[1..]),
        };
        let data = fs::read(path)?;
        let text = decode_string(&data)?;
        let mut args = cmd::native::parse(&text)?;
        result.append(&mut args);
    }

    Ok(result)
}

fn decode_string(data: &[u8]) -> crate::Result<String> {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Ok(String::from_utf8(data[3..].to_vec())?)
    } else if data.starts_with(&[0xFE, 0xFF]) {
        Ok(decode_utf16(&data[2..], |a, b| (a << 8) + b)?)
    } else if data.starts_with(&[0xFF, 0xFE]) {
        Ok(decode_utf16(&data[2..], |a, b| (b << 8) + a)?)
    } else {
        Ok(Encoding::ANSI.to_string(data)?)
    }
}

fn decode_utf16<F: Fn(u16, u16) -> u16>(data: &[u8], endian: F) -> crate::Result<String> {
    let mut utf16 = Vec::new();
    if data.len() % 2 != 0 {
        return Err(crate::Error::FromUtf16OddLength);
    }
    let mut i = 0;
    while i < data.len() {
        utf16.push(endian(u16::from(data[i]), u16::from(data[i + 1])));
        i += 2;
    }
    Ok(String::from_utf16(&utf16)?)
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
            ));
        })
        // Output to stdout and the log file in the temporary directory we made above to test
        .chain(io::stdout())
        .chain(fern::log_file(log_file).unwrap())
        // Only log messages Info and above
        .level(log::LevelFilter::Info)
        .apply()
        .expect("Failed to initialize logging");
}

pub enum ParamValue<T> {
    None,
    Single(T),
    Many(Vec<T>),
}

pub trait OsStrExt {
    fn concat(self, str: impl AsRef<OsStr>) -> OsString;
}

impl OsStrExt for OsString {
    fn concat(mut self, str: impl AsRef<OsStr>) -> OsString {
        self.push(str);
        self
    }
}

pub fn find_param<T, R, F: Fn(&T) -> Option<R>>(args: &[T], filter: F) -> ParamValue<R> {
    let mut found: Vec<R> = args.iter().filter_map(filter).collect();
    match found.len() {
        0 => ParamValue::None,
        1 => ParamValue::Single(found.pop().unwrap()),
        _ => ParamValue::Many(found),
    }
}

#[test]
fn test_hash_stream() {
    use std::io::Cursor;
    assert_eq!(
        hash_stream(&mut Cursor::new(b"foobar")).unwrap(),
        "c3ab8ff13720e8ad9047dd39466b3c8974e592c2fa383d4a3960714caef0c4f2".to_string()
    );
}

#[test]
fn test_decode_string() {
    // ANSI
    assert_eq!(&decode_string(b"test").unwrap(), "test");
    // UTF-8
    assert_eq!(
        &decode_string(b"\xEF\xBB\xBFtest \xD1\x80\xD1\x83\xD1\x81").unwrap(),
        "test рус"
    );
    // UTF-16LE
    assert_eq!(
        &decode_string(b"\xFF\xFEt\x00e\x00s\x00t\x00 \x00\x40\x04\x43\x04\x41\x04").unwrap(),
        "test рус"
    );
    // UTF-16BE
    assert_eq!(
        &decode_string(b"\xFE\xFF\x00t\x00e\x00s\x00t\x00 \x04\x40\x04\x43\x04\x41").unwrap(),
        "test рус"
    );
}
